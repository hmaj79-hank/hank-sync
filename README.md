# hank-sync — Minimal QUIC File Sync

Einfaches Tool zum Kopieren von Dateien über QUIC.

## Features

- **QUIC Transport** — schnell, NAT-freundlich, verschlüsselt
- **Server-Modus** — empfängt Dateien in konfigurierbares Root-Dir
- **Client-Modus** — sendet und holt Dateien/Verzeichnisse
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
hank-sync put --server 192.168.178.20:4433 myfile.txt

# Verzeichnis
hank-sync put --server 192.168.178.20:4433 ./my-project/

# Mit Ziel-Pfad
hank-sync put --server 192.168.178.20:4433 ./data/ --dest backup/2024/
```

### Datei holen (Download)

```bash
# Datei vom Server holen (im aktuellen Ordner speichern)
hank-sync get --server 192.168.178.20:4433 /path/auf/server.txt

# Datei in Zielpfad speichern
hank-sync get --server 192.168.178.20:4433 /path/auf/server.txt --dest ./downloads/
```

### Datei ansehen (Dump)

```bash
hank-sync view --server 192.168.178.20:4433 /path/auf/server.txt
```

### Navigieren (cwd im Client)

```bash
# listet aktuelles cwd (state.json)
hank-sync list --server 192.168.178.20:4433

# long list (ls -al)
hank-sync listl --server 192.168.178.20:4433

# recursive list (ls -R)
hank-sync listr --server 192.168.178.20:4433

# hoch (parent)
hank-sync up --server 192.168.178.20:4433

# zurück zum vorherigen Verzeichnis
hank-sync down --server 192.168.178.20:4433

# in Unterordner wechseln
hank-sync down --server 192.168.178.20:4433 logs
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
