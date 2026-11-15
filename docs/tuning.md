# Tuning performa & footprint

| Profil | Cocok untuk |
|--------|-------------|
| **Minimal footprint** (default) | Homelab, edge, VM kecil. Reservasi kecil, GC image agresif, Hubble mati. |
| **Balanced** | Produksi umum. |
| **Throughput** | Beban tinggi: buffer TCP besar, CPU governor *performance*, `cpuManagerPolicy=static`. |

## Yang diterapkan

- **OS**: swap mati, modul `overlay`/`br_netfilter`, sysctl (somaxconn, conntrack, inotify, file-max), THP = `madvise`, journald dibatasi 200 MB, layanan tak perlu dimatikan (snapd, ModemManager, cups, avahi, multipathd…).
- **containerd**: `SystemdCgroup=true`, unduhan paralel.
- **kubelet**: `systemReserved`/`kubeReserved` dihitung dari RAM node (rumus GKE), `evictionHard`, ambang GC image, `serializeImagePulls=false`.
- **etcd**: `quota-backend-bytes` 4 GiB.

Isi **RAM per node** di langkah Tuning agar reservasi kubelet akurat.
