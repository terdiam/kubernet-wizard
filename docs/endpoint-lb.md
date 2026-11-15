# Endpoint control plane & Service LoadBalancer

## Endpoint control plane

| Opsi | Kapan dipakai |
|------|---------------|
| **kube-vip** (direkomendasikan baremetal) | VIP via ARP/BGP dijalankan sebagai pod. Ringan, tanpa paket tambahan. |
| **keepalived + HAProxy** | Pola klasik. API diakses lewat port **8443** (HAProxy di node CP). |
| **Cloud load balancer** | Hanya target cloud. LB dibuat OpenTofu. |

VIP harus berupa **IP bebas** di subnet node, bukan alamat node mana pun.

## Service LoadBalancer

| Opsi | Catatan |
|------|---------|
| **MetalLB** | Standar baremetal. Mode L2 (ARP) paling mudah; BGP untuk router yang mendukung. |
| **kube-vip cloud provider** | Satu komponen untuk VIP + service. Cocok cluster kecil. |
| **Cloud CCM** | Gunakan LB native provider cloud. |

### MetalLB BGP {#metallb-bgp}

Mode BGP membutuhkan `BGPPeer` untuk router Anda. Wizard membuat pool + advertisement; tambahkan peer di repo GitOps atau `kubectl apply`.

Pool alamat: format `192.168.1.240-192.168.1.250` atau CIDR. Jangan tumpang tindih dengan VIP atau DHCP.
