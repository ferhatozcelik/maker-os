// Package network: NetworkManager D-Bus API'si ile ağ durumu, Wi-Fi tarama,
// bağlanma (başarısızlıkta önceki bağlantıya, o da olmazsa kurulum erişim
// noktasına dönüş) ve kayıtlı ağı unutma. NM yoksa durum /sys'ten okunur.
package network

import (
	"context"
	"net"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

// Kurulum erişim noktası (v1 ile aynı davranış; os/ tarafındaki fallback betiği aynı profili kullanmalı).
const (
	HotspotID   = "makeros-hotspot"
	HotspotSSID = "MakerOS"
	HotspotIP   = "10.42.0.1"
)

// Status, GET /net gövdesi.
type Status struct {
	Online bool    `json:"online"`
	Ifaces []Iface `json:"ifaces"`
	// WifiEnabled Wi-Fi radyosu açık mı (NM WirelessEnabled).
	WifiEnabled bool    `json:"wifi_enabled"`
	Hotspot     Hotspot `json:"hotspot"`
}

// Iface bir ağ arayüzü.
type Iface struct {
	Name      string   `json:"name"`
	Type      string   `json:"type"` // ethernet | wifi
	State     string   `json:"state"`
	IPv4      []string `json:"ipv4"`
	MAC       string   `json:"mac"`
	SpeedMbps *int     `json:"speed_mbps,omitempty"`
	SSID      string   `json:"ssid,omitempty"`
}

// Hotspot kurulum erişim noktasının durumu.
type Hotspot struct {
	Active bool   `json:"active"`
	SSID   string `json:"ssid"`
}

// AP tarama sonucu.
type AP struct {
	SSID     string `json:"ssid"`
	Signal   int    `json:"signal"`
	Security string `json:"security"` // open | wpa2 | wpa3 | enterprise
	Known    bool   `json:"known"`
	Active   bool   `json:"active"`
}

// EAP kurumsal ağ kimliği.
type EAP struct {
	Identity string `json:"identity"`
	Password string `json:"password"`
	Method   string `json:"method"` // peap (varsayılan) | ttls | pwd
}

// ConnectRequest, POST /net/wifi/connect gövdesi.
type ConnectRequest struct {
	SSID     string `json:"ssid"`
	Password string `json:"password,omitempty"`
	Hidden   bool   `json:"hidden,omitempty"`
	EAP      *EAP   `json:"eap,omitempty"`
}

// Progress, bağlanma işinin ilerleme bildirimidir.
type Progress func(pct int, line string)

// Provider ağ arka ucudur.
type Provider interface {
	Status(ctx context.Context) (Status, error)
	Scan(ctx context.Context) ([]AP, error)
	Connect(ctx context.Context, req ConnectRequest, p Progress) error
	Forget(ctx context.Context, ssid string) error
	// SetWifiEnabled Wi-Fi radyosunu açar/kapatır.
	SetWifiEnabled(ctx context.Context, enabled bool) error
}

// ValidSSID SSID 1-32 bayt ve NUL içermemeli.
func ValidSSID(s string) bool { return len(s) >= 1 && len(s) <= 32 && !strings.ContainsRune(s, 0) }

// DedupeAPs aynı SSID'li erişim noktalarından en güçlüsünü tutar, kurulum
// ağını çıkarır, etkin olanı başa ve sinyale göre sıralar.
func DedupeAPs(list []AP) []AP {
	best := map[string]AP{}
	for _, a := range list {
		if a.SSID == "" || a.SSID == HotspotSSID {
			continue
		}
		cur, ok := best[a.SSID]
		if !ok || a.Signal > cur.Signal {
			a.Active = a.Active || cur.Active
			a.Known = a.Known || cur.Known
			best[a.SSID] = a
		} else if a.Active {
			cur.Active = true
			best[a.SSID] = cur
		}
	}
	out := make([]AP, 0, len(best))
	for _, a := range best {
		out = append(out, a)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Active != out[j].Active {
			return out[i].Active
		}
		if out[i].Signal != out[j].Signal {
			return out[i].Signal > out[j].Signal
		}
		return out[i].SSID < out[j].SSID
	})
	return out
}

// SysfsStatus, NetworkManager yokken /sys/class/net ve /proc/net/route'tan durum üretir.
func SysfsStatus(root string) Status {
	st := Status{Ifaces: []Iface{}, Hotspot: Hotspot{SSID: HotspotSSID}}
	base := filepath.Join(root, "/sys/class/net")
	entries, _ := os.ReadDir(base)
	read := func(name, f string) string {
		b, _ := os.ReadFile(filepath.Join(base, name, f))
		return strings.TrimSpace(string(b))
	}
	for _, e := range entries {
		name := e.Name()
		if name == "lo" {
			continue
		}
		typ := ""
		if _, err := os.Stat(filepath.Join(base, name, "wireless")); err == nil {
			typ = "wifi"
		} else if read(name, "type") == "1" && !strings.HasPrefix(name, "veth") && !strings.HasPrefix(name, "docker") && !strings.HasPrefix(name, "br-") {
			if _, err := os.Stat(filepath.Join(base, name, "device")); err == nil || strings.HasPrefix(name, "eth") || strings.HasPrefix(name, "en") {
				typ = "ethernet"
			}
		}
		if typ == "" {
			continue
		}
		ifc := Iface{Name: name, Type: typ, MAC: read(name, "address"), IPv4: []string{}}
		switch read(name, "operstate") {
		case "up":
			ifc.State = "connected"
		case "down", "lowerlayerdown":
			ifc.State = "disconnected"
		default:
			ifc.State = "unavailable"
		}
		if typ == "ethernet" {
			if s, err := strconv.Atoi(read(name, "speed")); err == nil && s > 0 {
				ifc.SpeedMbps = &s
			}
		}
		if ni, err := net.InterfaceByName(name); err == nil {
			addrs, _ := ni.Addrs()
			for _, a := range addrs {
				if ipn, ok := a.(*net.IPNet); ok && ipn.IP.To4() != nil {
					ifc.IPv4 = append(ifc.IPv4, ipn.String())
					if ipn.IP.Equal(net.ParseIP(HotspotIP)) {
						st.Hotspot.Active = true
					}
				}
			}
		}
		if typ == "wifi" {
			st.WifiEnabled = true
		}
		st.Ifaces = append(st.Ifaces, ifc)
	}
	route, _ := os.ReadFile(filepath.Join(root, "/proc/net/route"))
	for _, l := range strings.Split(string(route), "\n")[1:] {
		if f := strings.Fields(l); len(f) > 2 && f[1] == "00000000" {
			st.Online = true
		}
	}
	return st
}
