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

- Token provider belum diset (mis. `TF_VAR_hcloud_token`).
- Jalankan ulang dari langkah Deploy → **Coba lagi**.

## Log

Setiap run tersimpan di `<project>/logs/deploy-<waktu>.log`.
