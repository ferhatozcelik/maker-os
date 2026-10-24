"""Merkez > Ekran: tarayicidaki noVNC baglantisini Merkez portu (80) uzerinden
yerel websockify'a (127.0.0.1:6080) aktarir. Boylece disariya yalnizca Merkez
portu acik kalir; masaustu ayri bir porttan yayinlanmaz.

WebSocket el sikismasi ve VNC sifresi websockify/x11vnc tarafinda kalir; bu
modul istegi oldugu gibi iletir ve iki yonlu baytlari tasir.
"""
import select
import socket

UPSTREAM = ("127.0.0.1", 6080)
BUF = 65536


def serve(handler):
    try:
        up = socket.create_connection(UPSTREAM, timeout=5)
    except OSError:
        return handler.send_json({"error": "Uzak ekran servisi (websockify) çalışmıyor."}, 503)
    handler.close_connection = True
    client = handler.connection
    try:
        # Istek satiri ve basliklar yeniden yazilir; yol websockify'in bekledigi
        # yola cevrilir. Cerez (Merkez kimligi) disari tasinmaz.
        lines = ["GET /websockify HTTP/1.1"]
        for k, v in handler.headers.items():
            if k.lower() != "cookie":
                lines.append("%s: %s" % (k, v))
        up.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("latin-1"))
        up.settimeout(None)
        client.settimeout(None)
        # WebSocket istemcisi 101 yanitini almadan veri gondermez; rfile
        # tamponunda bekleyen bayt olmaz, dogrudan sokete gecilir.
        socks = [client, up]
        while True:
            readable, _, broken = select.select(socks, [], socks, 60)
            if broken:
                break
            if not readable:
                continue
            for s in readable:
                data = s.recv(BUF)
                if not data:
                    return
                (up if s is client else client).sendall(data)
    except OSError:
        pass
    finally:
        up.close()
