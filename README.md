# hank-sync — Minimal QUIC File Sync

Einfaches Tool zum Kopieren von Dateien über QUIC.

## Features

- **QUIC Transport** — schnell, NAT-freundlich, verschlüsselt
- **Server-Modus** — empfängt Dateien in konfigurierbares Root-Dir
- **Client-Modus** — sendet Dateien/Verzeichnisse
- **Self-signed Certs** — automatisch generiert (später: hank-ca Integration)

## Usage

### Server starten (Linux)

```bash
# Empfängt Dateien nach /backup/incoming
hank-sync server --root /backup/incoming --bind 0.0.0.0:4433
```

### Datei senden (Windows → Linux)

```bash
# Einzelne Datei
hank-sync send --server 192.168.178.20:4433 myfile.txt

# Verzeichnis
hank-sync send --server 192.168.178.20:4433 ./my-project/

# Mit Ziel-Pfad
hank-sync send --server 192.168.178.20:4433 ./data/ --dest backup/2024/
```

### Status abfragen

```bash
hank-sync status --server 192.168.178.20:4433
```

## Konfiguration

```toml
# ~/.config/hank-sync/config.toml

[server]
root = "/backup/incoming"
bind = "0.0.0.0:4433"

[client]
default_server = "192.168.178.20:4433"

[tls]
# Später: Pfade zu hank-ca Zertifikaten
# cert = "/path/to/cert.pem"
# key = "/path/to/key.pem"
# ca = "/path/to/ca.pem"
```

## Protokoll

Einfaches Request/Response über QUIC Streams:

```
Client → Server: { "cmd": "put", "path": "foo/bar.txt", "size": 1234, "hash": "abc..." }
Server → Client: { "ok": true }
Client → Server: <file bytes>
Server → Client: { "ok": true, "written": 1234 }
```

## Roadmap

- [x] Projekt-Struktur
- [ ] Self-signed TLS
- [ ] Server: Dateien empfangen
- [ ] Client: Dateien senden
- [ ] Verzeichnisse rekursiv
- [ ] Resume bei Abbruch
- [ ] Delta-Sync (nur geänderte Bytes)
- [ ] hank-ca Integration
