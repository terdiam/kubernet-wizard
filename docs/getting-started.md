# Memulai

kubernet-wizard membantu Anda mendesain cluster Kubernetes langkah demi langkah, lalu **menghasilkan project OpenTofu + Ansible** dan **menjalankannya** langsung dari aplikasi.

## Alur singkat

1. **Home** — pilih *Cluster baru*, *Buka project*, atau mulai dari **preset**.
2. **Wizard** — 12 langkah: Project → Target → Distro → Topologi → Endpoint & LB → Network → Ingress → Fitur → GitOps → Tuning → Review → Deploy.
3. **Review** — lihat semua file yang akan dibuat sebelum apa pun dijalankan.
4. **Deploy** — log real-time per fase. Untuk cloud, rencana OpenTofu ditampilkan dan butuh konfirmasi.

## Prasyarat

| Tool | Kegunaan |
|------|----------|
| Ansible ≥ 2.16 | Bootstrap node & cluster |
| OpenTofu ≥ 1.8 | Hanya bila membuat VM di cloud |
| kubectl, Helm | Verifikasi & add-on |
| SSH key | Akses root/sudo ke node |

Windows: Ansible berjalan di **WSL2** (`wsl --install`, lalu `sudo apt install ansible` di dalam WSL).

## Mode Dasar vs Lanjutan

Mode **Dasar** hanya menampilkan pilihan penting dengan default aman. Mode **Lanjutan** membuka CIDR, MTU, mode kube-proxy, dan detail tuning.

## Demo mode

Belum punya server? Jalankan **Demo** di langkah Deploy: alur dan log disimulasikan tanpa menyentuh server mana pun.

## CLI

Semua fitur juga tersedia headless: `kw validate`, `kw generate`, `kw deploy`, `kw upgrade-ha`, `kw prereq`.

## Update aplikasi

- Aplikasi mengecek update otomatis tiap 24 jam (bisa dimatikan di ⚙ Pengaturan), atau manual lewat **Cek update sekarang**.
- Kanal **Stable** atau **Beta**. Beta juga menerima rilis stable.
- Setiap file update diverifikasi **SHA-256 + tanda tangan minisign** sebelum dipasang. File yang tidak valid dihapus dan tidak dipasang.
- Update tidak dipasang saat deploy sedang berjalan.
- Versi lama dicadangkan. Jika versi baru gagal start 3 kali, versi lama dipulihkan otomatis.
- Instalasi dari paket (deb/rpm) diperbarui lewat package manager, bukan dari aplikasi.
