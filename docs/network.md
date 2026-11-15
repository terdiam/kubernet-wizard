# Network (CNI, CIDR, kube-proxy)

## Pilihan CNI

| CNI | Kelebihan | Kekurangan |
|-----|-----------|------------|
| **Cilium** | eBPF, bisa menggantikan kube-proxy, Gateway API, observability | RAM lebih besar (~300 MiB/node) |
| **Calico** | NetworkPolicy matang, BGP | Lebih banyak komponen |
| **Flannel** | Paling ringan (~50 MiB) | Tanpa NetworkPolicy |
| **Canal** | Flannel + policy Calico (default RKE2) | — |

## CIDR

- **Pod CIDR** (default `10.42.0.0/16`): tiap node memakai /24 → /16 cukup untuk 256 node.
- **Service CIDR** (default `10.43.0.0/16`): maksimal /12.
- **Cluster DNS** otomatis = alamat ke-10 Service CIDR (mis. `10.43.0.10`).
- Ketiganya **tidak boleh** tumpang tindih dengan subnet node.

## kube-proxy

`iptables` (default), `ipvs` (cluster besar), `nftables` (kernel baru), atau **None** — hanya dengan Cilium (eBPF kube-proxy replacement, CPU lebih rendah).
