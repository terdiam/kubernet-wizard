# Troubleshooting

## SSH gagal / "UNREACHABLE"

- Pastikan `ssh <user>@<ip>` berhasil tanpa password (pakai key yang sama dengan inventory).
- User non-root perlu sudo tanpa password.

## VIP tidak menjawab

- Interface (`eth0`, `ens18`…) harus benar di langkah Endpoint.
- VIP harus di subnet yang sama dan tidak dipakai perangkat lain.

## Node NotReady

- CNI belum jalan: cek `kubectl -n kube-system get pods`.
- Pod CIDR tumpang tindih dengan jaringan node.

## OpenTofu gagal

- Kredensial provider belum diset di environment (mis. `HCLOUD_TOKEN`, `AWS_PROFILE`). Lihat tabel di **Getting started → Cloud**.
- Aplikasi yang dibuka dari Finder/menu tidak membaca `~/.zshrc`; jalankan dari terminal atau set variabel di sistem.
- Jalankan ulang dari langkah Deploy → **Coba lagi**.

## Log

Setiap run tersimpan di `<project>/logs/deploy-<waktu>.log`.
