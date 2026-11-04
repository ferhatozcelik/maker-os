// Package priv, makerd ile kök yetkili dar yardımcı makerd-priv arasındaki
// protokolü uygular. Unix soketi üzerinden bağlantı başına tek satır JSON
// istek ve tek satır JSON yanıt; komutlar sabit bir izin listesindedir.
package priv

import "encoding/json"

// Komut izin listesi.
const (
	CmdAuthCheck      = "auth.check"
	CmdSystemPassword = "system.password"
	CmdSystemHostname = "system.hostname"
	CmdPower          = "power"
	// CmdUserCreate ilk açılış sihirbazının yönetici hesabını oluşturur.
	// Kurulum tamamlandıktan sonra (setup-done dosyası varsa) reddedilir.
	CmdUserCreate = "user.create"
	// CmdOTAInstall imzalı RAUC paketini (yalnız OTADir altındaki .raucb) kurmaya başlatır; hemen döner.
	CmdOTAInstall = "ota.install"
	// CmdOTAStatus RAUC kurulum durumunu (işlem, ilerleme, son hata) döndürür.
	CmdOTAStatus = "ota.status"
)

// OTADir makerd'nin güncelleme paketlerini indirdiği dizindir (makerd StateDirectory altında).
const OTADir = "/var/lib/makerd/ota"

// DefaultSocket makerd-priv soketinin varsayılan yoludur.
const DefaultSocket = "/run/makeros/priv.sock"

// Request bir komut isteğidir.
type Request struct {
	Cmd  string          `json:"cmd"`
	Args json.RawMessage `json:"args,omitempty"`
}

// Response komut yanıtıdır.
type Response struct {
	OK    bool            `json:"ok"`
	Data  json.RawMessage `json:"data,omitempty"`
	Error *Error          `json:"error,omitempty"`
}

// Error, makerd-priv'in döndürdüğü hatadır (code: invalid, forbidden, not_found, internal).
type Error struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (e *Error) Error() string { return e.Code + ": " + e.Message }

// AuthCheckArgs auth.check argümanlarıdır.
type AuthCheckArgs struct {
	User     string `json:"user"`
	Password string `json:"password"`
}

// AuthCheckResult auth.check yanıtıdır.
type AuthCheckResult struct {
	OK bool `json:"ok"`
}

// PasswordArgs system.password argümanlarıdır. User boşsa çağıranın kullanıcısıdır.
type PasswordArgs struct {
	User string `json:"user,omitempty"`
	Old  string `json:"old"`
	New  string `json:"new"`
}

// HostnameArgs system.hostname argümanlarıdır.
type HostnameArgs struct {
	Hostname string `json:"hostname"`
}

// OTAInstallArgs ota.install argümanlarıdır.
type OTAInstallArgs struct {
	Path string `json:"path"`
}

// OTAStatus ota.status yanıtıdır (RAUC D-Bus Installer özellikleri).
type OTAStatus struct {
	Operation string `json:"operation"` // idle | installing
	Progress  int    `json:"progress"`  // 0-100
	Message   string `json:"message"`
	LastError string `json:"last_error"`
}

// PowerArgs power argümanlarıdır (reboot|poweroff|tryboot). tryboot: yeni kurulan slottan
// tek seferlik deneme açılışı (Pi firmware; açılmazsa bir sonraki açılışta eski slota döner).
type PowerArgs struct {
	Action string `json:"action"`
}

// UserCreateArgs user.create argümanlarıdır.
type UserCreateArgs struct {
	User     string `json:"user"`
	Password string `json:"password"`
	Fullname string `json:"fullname,omitempty"`
}

// UserCreateResult user.create yanıtıdır. Created=false: kurulumdan önce var
// olan (ör. kilitli) bir hesap güncellendi.
type UserCreateResult struct {
	UID     int  `json:"uid"`
	Created bool `json:"created"`
	// Service, makerd-user@<user>.service etkinleştirildi mi.
	Service bool `json:"service"`
}
