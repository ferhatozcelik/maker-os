package events

import (
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"testing"
	"time"
)

func recv(t *testing.T, c *Client) Message {
	t.Helper()
	select {
	case b := <-c.Send:
		var m Message
		if err := json.Unmarshal(b, &m); err != nil {
			t.Fatal(err)
		}
		return m
	case <-time.After(2 * time.Second):
		t.Fatal("no message")
	}
	return Message{}
}

func TestHubProducerLifecycle(t *testing.T) {
	h := NewHub(context.Background(), slog.New(slog.NewTextHandler(io.Discard, nil)), "jobs")
	n := 0
	h.Register("system", Every(20*time.Millisecond, func(context.Context) (any, error) { n++; return map[string]int{"n": n}, nil }))
	if h.Running("system") {
		t.Fatal("producer running without subscribers")
	}
	a, b := h.NewClient(), h.NewClient()
	h.Subscribe(a, []string{"system", "bogus"})
	if !h.Running("system") || h.Subscribers("system") != 1 || h.Subscribers("bogus") != 0 {
		t.Fatal("subscribe did not start producer")
	}
	if m := recv(t, a); m.Topic != "system" {
		t.Fatalf("topic %q", m.Topic)
	}
	h.Subscribe(b, []string{"system"})
	if m := recv(t, b); m.Topic != "system" { // son çerçeve veya yeni tik
		t.Fatalf("b topic %q", m.Topic)
	}
	h.Unsubscribe(a, []string{"system"})
	if !h.Running("system") {
		t.Fatal("stopped while b subscribed")
	}
	h.Remove(b)
	if h.Running("system") || h.Subscribers("system") != 0 {
		t.Fatal("producer still running")
	}
}

func TestHubPublishFiltering(t *testing.T) {
	h := NewHub(context.Background(), slog.New(slog.NewTextHandler(io.Discard, nil)), "jobs", "notifications")
	a, b := h.NewClient(), h.NewClient()
	h.Subscribe(a, []string{"jobs"})
	h.Subscribe(b, []string{"notifications"})
	h.Publish("jobs", map[string]string{"id": "1"})
	if m := recv(t, a); m.Topic != "jobs" {
		t.Fatal(m)
	}
	select {
	case <-b.Send:
		t.Fatal("b got jobs")
	default:
	}
	// Dolu kuyruk bloklamaz.
	for i := 0; i < 100; i++ {
		h.Publish("jobs", i)
	}
}

func TestOnChange(t *testing.T) {
	h := NewHub(context.Background(), slog.New(slog.NewTextHandler(io.Discard, nil)))
	v := 1
	h.Register("net", OnChange(10*time.Millisecond, func(context.Context) (any, error) { return v, nil }))
	c := h.NewClient()
	h.Subscribe(c, []string{"net"})
	recv(t, c)
	time.Sleep(60 * time.Millisecond)
	select {
	case <-c.Send:
		t.Fatal("published unchanged value")
	default:
	}
	h.Remove(c)
}
