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

Aplikasi memeriksa tool wajib setiap kali dibuka. Selama ada yang belum terpasang, layar **Lengkapi prasyarat** tidak bisa dilewati. Klik **Install** (atau **Install semua**) dan aplikasi memasangnya sendiri tanpa hak admin:

| Tool | Cara pasang otomatis |
|------|----------------------|
| kubectl, Helm, OpenTofu | Binary resmi (dl.k8s.io, get.helm.sh, rilis GitHub OpenTofu), dicek SHA-256 |
| Ansible | `ansible-core` di Python venv (butuh Python ≥ 3.10) |
| Git | macOS: dialog Command Line Tools · Windows: `winget` · Linux: `pkexec` + apt/dnf |
| OpenSSH | Linux: `pkexec` + apt/dnf · macOS & Windows: sudah bawaan / Optional features |

Tool disimpan di folder data aplikasi (`…/kubernet-wizard/tools`) dan dipakai lebih dulu daripada `PATH`. Dari terminal: `kw tools install <nama|all>`.

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

## Cloud (OpenTofu)

Pilih **Cloud → Create with OpenTofu**. Region, ukuran, dan image terisi default per provider (Ubuntu 24.04). Kredensial dibaca dari environment:

| Provider | Kredensial | Catatan |
|---|---|---|
| AWS | `AWS_PROFILE` atau `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Image: AMI id atau pola nama AMI (publisher resmi saja). LB: NLB. |
| Google Cloud | `gcloud auth application-default login` + `TF_VAR_project` | Field region diisi **zone** (mis. `europe-west4-a`). LB: proxy TCP. |
| Azure | `az login` + `ARM_SUBSCRIPTION_ID` | Image: `publisher:offer:sku:version`. LB: Standard LB. |
| Akamai / Linode | `LINODE_TOKEN` | LB: NodeBalancer. |
| DigitalOcean | `DIGITALOCEAN_TOKEN` | LB: DO Load Balancer. |
| Hetzner | `HCLOUD_TOKEN` | LB: Hetzner LB (jaringan privat). |
| Proxmox VE | `PROXMOX_VE_ENDPOINT` + `PROXMOX_VE_API_TOKEN` | Region = nama node, ukuran = `<cores>x<MiB>`, image = VM id template cloud-init. IP via DHCP (reservasi lease). Tanpa cloud LB: pakai kube-vip. |

Setiap provider dibuatkan firewall: SSH dan API hanya dari `admin_cidrs` (default semua — **persempit untuk produksi**), 80/443 terbuka untuk ingress, trafik antar node bebas. User login: `kw` (sudo) di AWS/GCP/Azure/Proxmox, `root` di Linode/DigitalOcean/Hetzner.

Azure + kubeadm: Azure LB tidak mendukung hairpin, sehingga kubelet di control plane yang lewat LB bisa sesekali gagal. Untuk Azure lebih disarankan k3s atau RKE2.
