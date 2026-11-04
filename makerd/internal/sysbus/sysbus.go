// Package sysbus, D-Bus sistem veriyoluna tek, tembel açılan bir bağlantı sağlar.
package sysbus

import (
	"sync"

	"github.com/godbus/dbus/v5"
)

var (
	mu   sync.Mutex
	conn *dbus.Conn
)

// Get, paylaşılan sistem veriyolu bağlantısını döndürür; kopmuşsa yeniden bağlanır.
func Get() (*dbus.Conn, error) {
	mu.Lock()
	defer mu.Unlock()
	if conn != nil && conn.Connected() {
		return conn, nil
	}
	c, err := dbus.ConnectSystemBus()
	if err != nil {
		return nil, err
	}
	conn = c
	return conn, nil
}
