# Endpoint control plane & Service LoadBalancer

## Endpoint control plane

| Opsi | Kapan dipakai |
|------|---------------|
| **kube-vip** (direkomendasikan baremetal) | VIP via ARP/BGP dijalankan sebagai pod. Ringan, tanpa paket tambahan. |
| **keepalived + HAProxy** | Pola klasik. API diakses lewat port **8443** (HAProxy di node CP). |
| **Cloud load balancer** | Hanya target cloud. LB dibuat OpenTofu. |
| **DNS name (no VIP)** | Jaringan yang tidak bisa memindahkan VIP: cloud VPC, VPS/dedicated hosting, VLAN tanpa IP bebas, switch yang memblokir ARP. |

VIP harus berupa **IP bebas** di subnet node, bukan alamat node mana pun.

### Kapan VIP tidak bisa dipakai {#no-vip}

kube-vip dan keepalived memindahkan satu IP antar node lewat ARP/VRRP. Ini butuh jaringan L2 datar dan satu IP kosong. Cloud publik (AWS, GCP, Azure, Linode, DigitalOcean, Hetzner) membuang ARP/VRRP, jadi wizard menonaktifkan kedua opsi itu untuk target cloud.

Pilih **DNS name (no VIP)** bila VIP tidak tersedia:

- Isi nama seperti `api.cluster.example.com`. Nama masuk SAN sertifikat, jadi tetap jadi endpoint saat cluster di-upgrade ke HA.
- Bila nama belum punya record DNS, wizard menulis `<IP control plane pertama> <nama>` di `/etc/hosts` tiap node agar bootstrap jalan.
- Setelah menambah control plane, arahkan record DNS ke semua control plane (round-robin) atau ke load balancer Anda sendiri.
- Di mesin operator, buat record DNS atau entri `/etc/hosts` sebelum memakai `kubeconfig`.

## Service LoadBalancer

| Opsi | Catatan |
|------|---------|
| **MetalLB** | Standar baremetal. Mode L2 (ARP) paling mudah; BGP untuk router yang mendukung. |
| **kube-vip cloud provider** | Satu komponen untuk VIP + service. Cocok cluster kecil. |
| **Cloud CCM** | Gunakan LB native provider cloud. |

### MetalLB BGP {#metallb-bgp}

Mode BGP membutuhkan `BGPPeer` untuk router Anda. Wizard membuat pool + advertisement; tambahkan peer di repo GitOps atau `kubectl apply`.

Pool alamat: format `192.168.1.240-192.168.1.250` atau CIDR. Jangan tumpang tindih dengan VIP atau DHCP.
