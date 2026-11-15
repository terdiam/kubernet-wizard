//! Minimal string table (Bahasa Indonesia + English) for UI chrome.
//! Technical labels (CNI names, distro names, ...) stay untranslated.

use kw_core::Step;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Id,
    En,
}

impl Lang {
    #[inline]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Id => Self::En,
            Self::En => Self::Id,
        }
    }

    #[inline]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Id => "ID",
            Self::En => "EN",
        }
    }
}

macro_rules! keys {
    ($($key:ident => $id:literal, $en:literal;)+) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum K { $($key),+ }

        #[inline]
        pub const fn t(lang: Lang, key: K) -> &'static str {
            match (lang, key) {
                $((Lang::Id, K::$key) => $id, (Lang::En, K::$key) => $en,)+
            }
        }
    };
}

keys! {
    Tagline => "Desain, generate, dan deploy cluster Kubernetes dengan aman.", "Design, generate and deploy Kubernetes clusters safely.";
    NewCluster => "Cluster baru", "New cluster";
    NewClusterHint => "Mulai dari default aman dan atur tiap langkah.", "Start from safe defaults and tune each step.";
    OpenProject => "Buka project", "Open project";
    OpenProjectHint => "Lanjutkan project tersimpan, Day-2 & upgrade HA.", "Continue a saved project, Day-2 & HA upgrade.";
    Docs => "Dokumentasi", "Docs";
    DocsHint => "Konsep, referensi field, dan troubleshooting.", "Concepts, field reference and troubleshooting.";
    Presets => "Mulai dari preset", "Start from a preset";
    Tour => "Tur", "Tour";
    Home => "Beranda", "Home";
    Back => "Kembali", "Back";
    Next => "Lanjut", "Next";
    Basic => "Dasar", "Basic";
    Advanced => "Lanjutan", "Advanced";
    Recommended => "Direkomendasikan", "Recommended";
    Learn => "Pelajari", "Learn more";
    Summary => "Ringkasan", "Summary";
    Footprint => "Perkiraan overhead sistem", "Estimated system overhead";
    WelcomeTitle => "Selamat datang di kubernet-wizard", "Welcome to kubernet-wizard";
    WelcomeBody => "Wizard ini menuntun Anda dari desain cluster sampai deploy: OpenTofu untuk infrastruktur, Ansible untuk bootstrap, GitOps untuk add-on. Semua file bisa direview sebelum dijalankan.", "This wizard guides you from cluster design to deployment: OpenTofu for infrastructure, Ansible for bootstrap, GitOps for add-ons. Every file can be reviewed before anything runs.";
    StartTour => "Mulai tur", "Start tour";
    Skip => "Lewati", "Skip";
    Prereq => "Prasyarat lokal", "Local prerequisites";
    Recheck => "Cek ulang", "Re-check";
    Generate => "Generate file", "Generate files";
    Deploy => "Deploy", "Deploy";
    Demo => "Demo (simulasi)", "Demo (simulated)";
    Cancel => "Batal", "Cancel";
    Retry => "Coba lagi", "Retry";
    ErrorsOnly => "Hanya error", "Errors only";
    FixErrors => "Perbaiki error sebelum lanjut", "Fix errors before continuing";
    SearchDocs => "Cari dokumentasi…", "Search docs…";
    ConfirmDeploy => "Jalankan di server sungguhan?", "Run against real servers?";
    ConfirmDeployBody => "Ansible akan mengubah konfigurasi node (paket, sysctl, layanan) dan memasang Kubernetes. Pastikan inventory benar.", "Ansible will change node configuration (packages, sysctl, services) and install Kubernetes. Make sure the inventory is correct.";
    ConfirmApply => "Terapkan rencana OpenTofu?", "Apply the OpenTofu plan?";
    ConfirmApplyBody => "Resource cloud akan dibuat dan dapat menimbulkan biaya. Tinjau output plan di log sebelum melanjutkan.", "Cloud resources will be created and may incur cost. Review the plan output in the log before continuing.";
    UpgradeHa => "Upgrade ke HA", "Upgrade to HA";
    UpgradeHaHint => "Tambahkan control plane baru (satu per baris: nama ip cp [user] [key]). Endpoint stabil sudah ada sejak awal, jadi tidak perlu rebuild.", "Add new control planes (one per line: name ip cp [user] [key]). The stable endpoint exists from day one, so no rebuild is needed.";
    TourDone => "Selesai", "Done";
    Settings => "Pengaturan", "Settings";
    UpdateAvailable => "Update tersedia", "Update available";
    UpdateDownload => "Unduh update", "Download update";
    UpdateRestart => "Restart & update", "Restart & update";
    UpdateViaPackage => "Terpasang lewat paket sistem: pasang .deb/.rpm baru dari halaman rilis.", "Installed from a system package: install the new .deb/.rpm from the release page.";
    UpdateOpenRelease => "Buka halaman rilis", "Open release page";
    UpdateCheckNow => "Cek update sekarang", "Check for updates now";
    UpdateChecking => "Mengecek update…", "Checking for updates…";
    UpdateUpToDate => "Sudah versi terbaru", "You're up to date";
    UpdateAutoCheck => "Cek update otomatis (tiap 24 jam)", "Check for updates automatically (every 24h)";
    UpdateAutoDownload => "Unduh update otomatis", "Download updates automatically";
    UpdateChannel => "Kanal rilis", "Release channel";
    UpdateBusy => "Deploy sedang berjalan. Update dipasang setelah selesai.", "A deploy is running. Install the update after it finishes.";
    UpdateVerified => "Terverifikasi (SHA-256 + tanda tangan minisign)", "Verified (SHA-256 + minisign signature)";
    UpdateInstalled => "Diperbarui ke versi", "Updated to version";
    CurrentVersion => "Versi saat ini", "Current version";
}

pub const fn step_title(lang: Lang, step: Step) -> &'static str {
    match (lang, step) {
        (Lang::Id, Step::Project) => "Project",
        (Lang::Id, Step::Target) => "Target",
        (Lang::Id, Step::Distro) => "Distro",
        (Lang::Id, Step::Topology) => "Topologi",
        (Lang::Id, Step::Endpoint) => "Endpoint & LB",
        (Lang::Id, Step::Network) => "Network",
        (Lang::Id, Step::Ingress) => "Ingress",
        (Lang::Id, Step::Features) => "Fitur",
        (Lang::Id, Step::GitOps) => "GitOps",
        (Lang::Id, Step::Tuning) => "Tuning",
        (Lang::Id, Step::Review) => "Review",
        (Lang::Id, Step::Deploy) => "Deploy",
        (Lang::En, Step::Project) => "Project",
        (Lang::En, Step::Target) => "Target",
        (Lang::En, Step::Distro) => "Distro",
        (Lang::En, Step::Topology) => "Topology",
        (Lang::En, Step::Endpoint) => "Endpoint & LB",
        (Lang::En, Step::Network) => "Network",
        (Lang::En, Step::Ingress) => "Ingress",
        (Lang::En, Step::Features) => "Features",
        (Lang::En, Step::GitOps) => "GitOps",
        (Lang::En, Step::Tuning) => "Tuning",
        (Lang::En, Step::Review) => "Review",
        (Lang::En, Step::Deploy) => "Deploy",
    }
}
