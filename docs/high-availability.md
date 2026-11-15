# High Availability & upgrade ke HA

## Apa itu HA?

Control plane HA = **3 atau 5** node control plane dengan etcd. Jumlah ganjil menjaga *quorum*: 3 node tahan 1 node mati, 5 node tahan 2.

## Single control plane yang siap HA

Walau HA dimatikan, wizard **selalu** membangun cluster dengan:

- **Endpoint stabil** (VIP kube-vip/keepalived atau cloud LB) sebagai alamat API, masuk SAN sertifikat.
- **Datastore etcd** (k3s `cluster-init`, RKE2 etcd, kubeadm stacked etcd), bukan SQLite.

Karena itu upgrade ke HA nanti cukup *menambah* control plane — tanpa membangun ulang cluster.

## Cara upgrade

1. Buka project (Home → Buka project).
2. Langkah **Deploy** → bagian **Upgrade ke HA**.
3. Masukkan 2 node baru (format sama seperti inventory), klik **Upgrade**.
4. Wizard menjalankan `upgrade-ha.yml`: siapkan node → join satu per satu → tunggu etcd 3 member → smoke test.

CLI: edit `cluster.yaml` (ha: true, control_planes: 3, tambah node) lalu `kw upgrade-ha --out <dir>`.
