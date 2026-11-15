# Ingress / Gateway & GitOps

## Ingress / Gateway

| Opsi | Catatan |
|------|---------|
| **Traefik** | Ringan; bawaan k3s. |
| **Envoy Gateway** | Gateway API modern, direkomendasikan produksi. |
| **Cilium Gateway API** | Tanpa controller tambahan bila CNI = Cilium. |
| **NGINX Gateway Fabric** | Gateway API dari NGINX. |
| **HAProxy Ingress**, **Contour** | Alternatif matang. |

ingress-nginx tidak ditawarkan karena proyeknya sudah *retired* (Maret 2026).

## GitOps

| Tool | Catatan |
|------|---------|
| **Argo CD** | UI web, app-of-apps. |
| **Flux** | Lebih ringan, tanpa UI. |

Saat GitOps aktif, add-on Helm **tidak** dipasang Ansible, melainkan ditulis ke `gitops/<path>/addons.yaml`. Push folder itu ke repo Anda; Argo CD/Flux yang menyinkronkan.

Repo privat: tambahkan deploy key (SSH) di provider Git dan buat secret kredensial repo di cluster.
