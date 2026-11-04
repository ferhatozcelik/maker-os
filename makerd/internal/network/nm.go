package network

import (
	"bytes"
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/sysbus"
	"github.com/godbus/dbus/v5"
)

const (
	nmDest     = "org.freedesktop.NetworkManager"
	nmPath     = "/org/freedesktop/NetworkManager"
	nmSettings = "/org/freedesktop/NetworkManager/Settings"
	ifNM       = nmDest
	ifDevice   = nmDest + ".Device"
	ifWired    = nmDest + ".Device.Wired"
	ifWireless = nmDest + ".Device.Wireless"
	ifAP       = nmDest + ".AccessPoint"
	ifIP4      = nmDest + ".IP4Config"
	ifSettings = nmDest + ".Settings"
	ifConn     = nmDest + ".Settings.Connection"
	ifActive   = nmDest + ".Connection.Active"

	devEthernet = 1
	devWifi     = 2
	modeAP      = 3

	secPSK   = 0x100
	sec8021X = 0x200
	secSAE   = 0x400

	activeActivated   = 2
	activeDeactivated = 4
)

// Settings NM bağlantı ayarlarıdır (a{sa{sv}}).
type settings = map[string]map[string]dbus.Variant

// NM, NetworkManager D-Bus arka ucudur.
type NM struct {
	Log  *slog.Logger
	Root string // sysfs yedeği için ("/")
	mu   sync.Mutex
}

func (n *NM) bus() (*dbus.Conn, error) {
	c, err := sysbus.Get()
	if err != nil {
		return nil, apierr.Unavailable("NetworkManager'a ulaşılamadı (D-Bus yok).")
	}
	var owner string
	if c.BusObject().Call("org.freedesktop.DBus.GetNameOwner", 0, nmDest).Store(&owner) != nil {
		return nil, apierr.Unavailable("NetworkManager çalışmıyor.")
	}
	return c, nil
}

func prop[T any](c *dbus.Conn, path dbus.ObjectPath, name string) (T, error) {
	var zero T
	v, err := c.Object(nmDest, path).GetProperty(name)
	if err != nil {
		return zero, err
	}
	t, ok := v.Value().(T)
	if !ok {
		return zero, fmt.Errorf("%s: beklenmeyen tür %T", name, v.Value())
	}
	return t, nil
}

func call(ctx context.Context, c *dbus.Conn, path dbus.ObjectPath, method string, args ...any) *dbus.Call {
	return c.Object(nmDest, path).CallWithContext(ctx, method, 0, args...)
}

func devState(s uint32) string {
	switch {
	case s == 10:
		return "unmanaged"
	case s == 20:
		return "unavailable"
	case s == 30:
		return "disconnected"
	case s >= 40 && s < 100:
		return "connecting"
	case s == 100:
		return "connected"
	case s == 110:
		return "disconnecting"
	case s == 120:
		return "failed"
	}
	return "unknown"
}

// Status, NM'den durum; NM yoksa sysfs.
func (n *NM) Status(ctx context.Context) (Status, error) {
	c, err := n.bus()
	if err != nil {
		return SysfsStatus(n.Root), nil
	}
	st := Status{Ifaces: []Iface{}, Hotspot: Hotspot{SSID: HotspotSSID}}
	state, _ := prop[uint32](c, nmPath, ifNM+".State")
	conn, _ := prop[uint32](c, nmPath, ifNM+".Connectivity")
	st.Online = conn == 4 || (conn == 0 && state == 70)
	st.WifiEnabled, _ = prop[bool](c, nmPath, ifNM+".WirelessEnabled")
	var devs []dbus.ObjectPath
	if err := call(ctx, c, nmPath, ifNM+".GetDevices").Store(&devs); err != nil {
		return SysfsStatus(n.Root), nil
	}
	for _, d := range devs {
		typ, _ := prop[uint32](c, d, ifDevice+".DeviceType")
		if typ != devEthernet && typ != devWifi {
			continue
		}
		ifc := Iface{IPv4: []string{}}
		ifc.Name, _ = prop[string](c, d, ifDevice+".Interface")
		s, _ := prop[uint32](c, d, ifDevice+".State")
		ifc.State = devState(s)
		ifc.MAC, _ = prop[string](c, d, ifDevice+".HwAddress")
		if ip4, err := prop[dbus.ObjectPath](c, d, ifDevice+".Ip4Config"); err == nil && ip4 != "/" {
			if ad, err := prop[[]map[string]dbus.Variant](c, ip4, ifIP4+".AddressData"); err == nil {
				for _, a := range ad {
					addr, _ := a["address"].Value().(string)
					pfx, _ := a["prefix"].Value().(uint32)
					if addr != "" {
						ifc.IPv4 = append(ifc.IPv4, fmt.Sprintf("%s/%d", addr, pfx))
					}
				}
			}
		}
		if typ == devEthernet {
			ifc.Type = "ethernet"
			if sp, err := prop[uint32](c, d, ifWired+".Speed"); err == nil && sp > 0 {
				v := int(sp)
				ifc.SpeedMbps = &v
			}
		} else {
			ifc.Type = "wifi"
			mode, _ := prop[uint32](c, d, ifWireless+".Mode")
			if ap, err := prop[dbus.ObjectPath](c, d, ifWireless+".ActiveAccessPoint"); err == nil && ap != "/" {
				ssid, _ := prop[[]byte](c, ap, ifAP+".Ssid")
				ifc.SSID = string(ssid)
			}
			if mode == modeAP {
				st.Hotspot.Active = true
				if ifc.SSID != "" {
					st.Hotspot.SSID = ifc.SSID
				}
			}
		}
		st.Ifaces = append(st.Ifaces, ifc)
	}
	return st, nil
}

func (n *NM) wifiDevice(ctx context.Context, c *dbus.Conn) (dbus.ObjectPath, string, error) {
	var devs []dbus.ObjectPath
	if err := call(ctx, c, nmPath, ifNM+".GetDevices").Store(&devs); err != nil {
		return "", "", apierr.Unavailable("Ağ aygıtları okunamadı.")
	}
	var first dbus.ObjectPath
	var firstName string
	for _, d := range devs {
		if t, _ := prop[uint32](c, d, ifDevice+".DeviceType"); t != devWifi {
			continue
		}
		name, _ := prop[string](c, d, ifDevice+".Interface")
		if name == "wlan0" {
			return d, name, nil
		}
		if first == "" {
			first, firstName = d, name
		}
	}
	if first == "" {
		return "", "", apierr.Unavailable("Wi-Fi aygıtı bulunamadı. Adaptörü, sürücüyü ve rfkill durumunu kontrol edin.")
	}
	return first, firstName, nil
}

type savedConn struct {
	path dbus.ObjectPath
	id   string
	ssid string
	mode string
}

func (n *NM) savedWifi(ctx context.Context, c *dbus.Conn) []savedConn {
	var paths []dbus.ObjectPath
	if call(ctx, c, nmSettings, ifSettings+".ListConnections").Store(&paths) != nil {
		return nil
	}
	var out []savedConn
	for _, p := range paths {
		var s settings
		if call(ctx, c, p, ifConn+".GetSettings").Store(&s) != nil {
			continue
		}
		if t, _ := s["connection"]["type"].Value().(string); t != "802-11-wireless" {
			continue
		}
		sc := savedConn{path: p}
		sc.id, _ = s["connection"]["id"].Value().(string)
		ssid, _ := s["802-11-wireless"]["ssid"].Value().([]byte)
		sc.ssid = string(ssid)
		sc.mode, _ = s["802-11-wireless"]["mode"].Value().(string)
		out = append(out, sc)
	}
	return out
}

func isHotspot(s savedConn) bool { return s.id == HotspotID || s.mode == "ap" }

func (n *NM) inAPMode(c *dbus.Conn, dev dbus.ObjectPath) bool {
	m, _ := prop[uint32](c, dev, ifWireless+".Mode")
	return m == modeAP
}

func (n *NM) radioOn(c *dbus.Conn) {
	_ = c.Object(nmDest, nmPath).SetProperty(ifNM+".WirelessEnabled", dbus.MakeVariant(true))
}

// SetWifiEnabled NM WirelessEnabled özelliğini ayarlar (polkit: enable-disable-wifi).
func (n *NM) SetWifiEnabled(ctx context.Context, enabled bool) error {
	c, err := n.bus()
	if err != nil {
		return err
	}
	call := c.Object(nmDest, nmPath).CallWithContext(ctx, "org.freedesktop.DBus.Properties.Set", 0,
		ifNM, "WirelessEnabled", dbus.MakeVariant(enabled))
	if call.Err != nil {
		var de dbus.Error
		if errors.As(call.Err, &de) && (strings.HasSuffix(de.Name, "PermissionDenied") || strings.HasSuffix(de.Name, "AccessDenied")) {
			return apierr.Forbidden("Wi-Fi radyosu değiştirilemedi: yetki yok.")
		}
		return apierr.Unavailable("Wi-Fi radyosu değiştirilemedi: " + call.Err.Error())
	}
	return nil
}

func (n *NM) deactivateDevice(ctx context.Context, c *dbus.Conn, dev dbus.ObjectPath) {
	if ac, err := prop[dbus.ObjectPath](c, dev, ifDevice+".ActiveConnection"); err == nil && ac != "/" {
		_ = call(ctx, c, nmPath, ifNM+".DeactivateConnection", ac).Err
	}
}

// Scan tarar. Tek radyo erişim noktası modundayken tarayamaz: kurulum ağı
// birkaç saniye kapatılır ve tarama sonunda geri açılır.
func (n *NM) Scan(ctx context.Context) ([]AP, error) {
	n.mu.Lock()
	defer n.mu.Unlock()
	c, err := n.bus()
	if err != nil {
		return nil, err
	}
	dev, _, err := n.wifiDevice(ctx, c)
	if err != nil {
		return nil, err
	}
	if n.inAPMode(c, dev) {
		n.deactivateDevice(ctx, c, dev)
		time.Sleep(2 * time.Second)
		defer func() {
			if err := n.activateHotspot(context.WithoutCancel(ctx), c, dev); err != nil {
				n.Log.Error("tarama sonrası erişim noktası açılamadı", "err", err)
			}
		}()
	}
	n.radioOn(c)
	before, _ := prop[int64](c, dev, ifWireless+".LastScan")
	_ = call(ctx, c, dev, ifWireless+".RequestScan", map[string]dbus.Variant{}).Err
	deadline := time.Now().Add(12 * time.Second)
	for time.Now().Before(deadline) {
		if ls, _ := prop[int64](c, dev, ifWireless+".LastScan"); ls != before {
			break
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(400 * time.Millisecond):
		}
	}
	known := map[string]bool{}
	for _, s := range n.savedWifi(ctx, c) {
		if !isHotspot(s) {
			known[s.ssid] = true
		}
	}
	active, _ := prop[dbus.ObjectPath](c, dev, ifWireless+".ActiveAccessPoint")
	var aps []dbus.ObjectPath
	if err := call(ctx, c, dev, ifWireless+".GetAllAccessPoints").Store(&aps); err != nil {
		return nil, apierr.Unavailable("Wi-Fi taraması başarısız oldu.")
	}
	list := make([]AP, 0, len(aps))
	for _, p := range aps {
		ssid, _ := prop[[]byte](c, p, ifAP+".Ssid")
		strength, _ := prop[byte](c, p, ifAP+".Strength")
		flags, _ := prop[uint32](c, p, ifAP+".Flags")
		wpa, _ := prop[uint32](c, p, ifAP+".WpaFlags")
		rsn, _ := prop[uint32](c, p, ifAP+".RsnFlags")
		list = append(list, AP{SSID: string(ssid), Signal: int(strength), Security: Security(flags, wpa, rsn),
			Known: known[string(ssid)], Active: p == active})
	}
	return DedupeAPs(list), nil
}

// Security NM AP bayraklarını sözleşme değerine çevirir.
func Security(flags, wpa, rsn uint32) string {
	all := wpa | rsn
	switch {
	case all&sec8021X != 0:
		return "enterprise"
	case rsn&secSAE != 0 && all&secPSK == 0:
		return "wpa3"
	case all&(secPSK|secSAE) != 0:
		return "wpa2"
	case flags&1 != 0:
		return "wpa2" // WEP: eski; parola ister
	}
	return "open"
}

func newUUID() string {
	b := make([]byte, 16)
	_, _ = rand.Read(b)
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:])
}

// apKeyMgmt, görünür AP'nin bayraklarından PSK/SAE seçimi.
func (n *NM) apKeyMgmt(ctx context.Context, c *dbus.Conn, dev dbus.ObjectPath, ssid string) string {
	var aps []dbus.ObjectPath
	_ = call(ctx, c, dev, ifWireless+".GetAllAccessPoints").Store(&aps)
	for _, p := range aps {
		s, _ := prop[[]byte](c, p, ifAP+".Ssid")
		if !bytes.Equal(s, []byte(ssid)) {
			continue
		}
		rsn, _ := prop[uint32](c, p, ifAP+".RsnFlags")
		wpa, _ := prop[uint32](c, p, ifAP+".WpaFlags")
		if (rsn|wpa)&secPSK == 0 && rsn&secSAE != 0 {
			return "sae"
		}
	}
	return "wpa-psk"
}

func wifiSettings(req ConnectRequest, keyMgmt string) settings {
	s := settings{
		"connection": {
			"id": dbus.MakeVariant(req.SSID), "uuid": dbus.MakeVariant(newUUID()),
			"type": dbus.MakeVariant("802-11-wireless"), "autoconnect": dbus.MakeVariant(true),
		},
		"802-11-wireless": {
			"ssid": dbus.MakeVariant([]byte(req.SSID)), "mode": dbus.MakeVariant("infrastructure"),
		},
		"ipv4": {"method": dbus.MakeVariant("auto")},
		"ipv6": {"method": dbus.MakeVariant("auto")},
	}
	if req.Hidden {
		s["802-11-wireless"]["hidden"] = dbus.MakeVariant(true)
	}
	switch {
	case req.EAP != nil:
		method := req.EAP.Method
		if method == "" {
			method = "peap"
		}
		s["802-11-wireless-security"] = map[string]dbus.Variant{"key-mgmt": dbus.MakeVariant("wpa-eap")}
		x := map[string]dbus.Variant{
			"eap": dbus.MakeVariant([]string{method}), "identity": dbus.MakeVariant(req.EAP.Identity),
			"password": dbus.MakeVariant(req.EAP.Password),
		}
		if method == "peap" || method == "ttls" {
			x["phase2-auth"] = dbus.MakeVariant("mschapv2")
		}
		s["802-1x"] = x
	case req.Password != "":
		s["802-11-wireless-security"] = map[string]dbus.Variant{
			"key-mgmt": dbus.MakeVariant(keyMgmt), "psk": dbus.MakeVariant(req.Password),
		}
	}
	return s
}

func hotspotSettings(iface string) settings {
	return settings{
		"connection": {
			"id": dbus.MakeVariant(HotspotID), "uuid": dbus.MakeVariant(newUUID()),
			"type": dbus.MakeVariant("802-11-wireless"), "autoconnect": dbus.MakeVariant(false),
			"interface-name": dbus.MakeVariant(iface),
		},
		"802-11-wireless": {
			"ssid": dbus.MakeVariant([]byte(HotspotSSID)), "mode": dbus.MakeVariant("ap"),
			"band": dbus.MakeVariant("bg"), "channel": dbus.MakeVariant(uint32(6)),
		},
		"ipv4": {
			"method": dbus.MakeVariant("shared"),
			"address-data": dbus.MakeVariant([]map[string]dbus.Variant{{
				"address": dbus.MakeVariant(HotspotIP), "prefix": dbus.MakeVariant(uint32(24)),
			}}),
		},
		"ipv6": {"method": dbus.MakeVariant("ignore")},
	}
}

// activateHotspot kurulum ağı profilini (yoksa oluşturup) etkinleştirir.
func (n *NM) activateHotspot(ctx context.Context, c *dbus.Conn, dev dbus.ObjectPath) error {
	var path dbus.ObjectPath
	for _, s := range n.savedWifi(ctx, c) {
		if s.id == HotspotID {
			path = s.path
		}
	}
	if path == "" {
		iface, _ := prop[string](c, dev, ifDevice+".Interface")
		if err := call(ctx, c, nmSettings, ifSettings+".AddConnection", hotspotSettings(iface)).Store(&path); err != nil {
			return fmt.Errorf("erişim noktası profili oluşturulamadı: %w", err)
		}
	}
	var ac dbus.ObjectPath
	if err := call(ctx, c, nmPath, ifNM+".ActivateConnection", path, dev, dbus.ObjectPath("/")).Store(&ac); err != nil {
		return err
	}
	return n.waitActive(ctx, c, ac, 30*time.Second)
}

func (n *NM) waitActive(ctx context.Context, c *dbus.Conn, ac dbus.ObjectPath, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		st, err := prop[uint32](c, ac, ifActive+".State")
		if err != nil {
			return errors.New("bağlantı sonlandı (parola, sinyal veya DHCP sorunu olabilir)")
		}
		switch st {
		case activeActivated:
			return nil
		case activeDeactivated:
			return errors.New("bağlantı reddedildi (parolayı kontrol edin)")
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(500 * time.Millisecond):
		}
	}
	return errors.New("bağlantı zaman aşımına uğradı")
}

// Connect, bağlantı mantığını NM D-Bus ile uygular:
//   - kayıtlı profil ve yeni parola yoksa profil etkinleştirilir;
//   - yeni parola/kimlik varsa yeni profil eklenir, başarıda eskisi silinir
//     (başarısızlıkta eski parola korunur);
//   - başarısızlıkta önceki bağlantı, o da olmazsa kurulum ağı (MakerOS) açılır.
func (n *NM) Connect(ctx context.Context, req ConnectRequest, p Progress) error {
	if !ValidSSID(req.SSID) {
		return apierr.Invalid("Ağ adı 1–32 bayt olmalıdır; Türkçe karakterler birden çok bayt kullanabilir.")
	}
	n.mu.Lock()
	defer n.mu.Unlock()
	c, err := n.bus()
	if err != nil {
		return err
	}
	dev, iface, err := n.wifiDevice(ctx, c)
	if err != nil {
		return err
	}
	var previous dbus.ObjectPath
	if ac, err := prop[dbus.ObjectPath](c, dev, ifDevice+".ActiveConnection"); err == nil && ac != "/" {
		previous, _ = prop[dbus.ObjectPath](c, ac, ifActive+".Connection")
	}
	wasAP := n.inAPMode(c, dev)
	p(10, req.SSID+" ağına bağlanılıyor…")
	n.radioOn(c)
	if wasAP {
		p(15, "Kurulum ağı (MakerOS) kapatılıyor…")
		n.deactivateDevice(ctx, c, dev)
		previous = ""
		time.Sleep(time.Second)
	}
	var existing []savedConn
	for _, s := range n.savedWifi(ctx, c) {
		if s.ssid == req.SSID && !isHotspot(s) {
			existing = append(existing, s)
		}
	}
	var created dbus.ObjectPath
	var ac dbus.ObjectPath
	if len(existing) > 0 && req.Password == "" && req.EAP == nil {
		p(30, "Kayıtlı profil etkinleştiriliyor…")
		err = call(ctx, c, nmPath, ifNM+".ActivateConnection", existing[0].path, dev, dbus.ObjectPath("/")).Store(&ac)
	} else {
		p(30, "Yeni profil oluşturuluyor…")
		km := n.apKeyMgmt(ctx, c, dev, req.SSID)
		err = call(ctx, c, nmPath, ifNM+".AddAndActivateConnection", wifiSettings(req, km), dev, dbus.ObjectPath("/")).Store(&created, &ac)
	}
	if err == nil {
		p(50, "Kimlik doğrulanıyor ve adres alınıyor…")
		err = n.waitActive(ctx, c, ac, 45*time.Second)
	}
	if err == nil {
		if created != "" {
			for _, s := range existing {
				_ = call(ctx, c, s.path, ifConn+".Delete").Err
			}
		}
		p(100, req.SSID+" ağına bağlanıldı ("+iface+").")
		return nil
	}
	// Geri dönüş.
	bg := context.WithoutCancel(ctx)
	if created != "" {
		_ = call(bg, c, created, ifConn+".Delete").Err
	}
	detail := "Ağa bağlanılamadı: " + err.Error()
	p(80, "Bağlantı başarısız; önceki bağlantı geri yükleniyor…")
	rerr := errors.New("önceki bağlantı yok")
	if previous != "" {
		var pac dbus.ObjectPath
		if rerr = call(bg, c, nmPath, ifNM+".ActivateConnection", previous, dev, dbus.ObjectPath("/")).Store(&pac); rerr == nil {
			rerr = n.waitActive(bg, c, pac, 25*time.Second)
		}
	}
	if rerr != nil {
		p(90, "Kurulum ağı (MakerOS) açılıyor…")
		if herr := n.activateHotspot(bg, c, dev); herr != nil {
			return fmt.Errorf("%s\nGeri dönüş de başarısız: %v", detail, herr)
		}
		return errors.New(detail + "\nKurulum ağı (MakerOS) açıldı; http://" + HotspotIP + "/ adresinden tekrar deneyin.")
	}
	return errors.New(detail + "\nÖnceki bağlantı geri açıldı.")
}

// Forget, SSID'ye ait kayıtlı profilleri siler.
func (n *NM) Forget(ctx context.Context, ssid string) error {
	n.mu.Lock()
	defer n.mu.Unlock()
	c, err := n.bus()
	if err != nil {
		return err
	}
	found := false
	for _, s := range n.savedWifi(ctx, c) {
		if s.ssid == ssid && !isHotspot(s) {
			found = true
			if err := call(ctx, c, s.path, ifConn+".Delete").Err; err != nil {
				return apierr.Internal("Ağ profili silinemedi: " + err.Error())
			}
		}
	}
	if !found {
		return apierr.NotFound("Kayıtlı Wi-Fi profili bulunamadı: " + ssid)
	}
	return nil
}
