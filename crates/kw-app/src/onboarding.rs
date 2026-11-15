//! Guided tour (coach marks). Targets are element ids that draw a highlight
//! ring while their step is active; the tour card itself is rendered by the
//! app shell.

use crate::i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Wizard,
    Docs,
}

#[derive(Debug, Clone, Copy)]
pub struct TourStep {
    pub target: &'static str,
    pub screen: Screen,
    pub title: [&'static str; 2],
    pub body: [&'static str; 2],
}

impl TourStep {
    #[inline]
    pub const fn title(&self, lang: Lang) -> &'static str {
        self.title[lang as usize]
    }

    #[inline]
    pub const fn body(&self, lang: Lang) -> &'static str {
        self.body[lang as usize]
    }
}

pub const MAIN_TOUR: [TourStep; 8] = [
    TourStep {
        target: "home-presets",
        screen: Screen::Home,
        title: ["Preset cepat", "Quick presets"],
        body: [
            "Pilih preset untuk mengisi seluruh wizard dengan konfigurasi teruji. Anda tetap bisa mengubah semuanya.",
            "Pick a preset to fill the whole wizard with a tested configuration. You can still change everything.",
        ],
    },
    TourStep {
        target: "stepper",
        screen: Screen::Wizard,
        title: ["Langkah wizard", "Wizard steps"],
        body: [
            "Status tiap langkah: ✓ valid, ⚠ peringatan, ✕ error. Klik untuk berpindah langkah kapan saja.",
            "Each step shows its status: ✓ valid, ⚠ warning, ✕ error. Click to jump between steps anytime.",
        ],
    },
    TourStep {
        target: "mode-toggle",
        screen: Screen::Wizard,
        title: ["Mode Dasar / Lanjutan", "Basic / Advanced mode"],
        body: [
            "Mode Dasar menyembunyikan opsi teknis (CIDR, MTU, kube-proxy) dengan default aman. Buka Lanjutan bila perlu.",
            "Basic mode hides technical options (CIDR, MTU, kube-proxy) behind safe defaults. Switch to Advanced when needed.",
        ],
    },
    TourStep {
        target: "content",
        screen: Screen::Wizard,
        title: ["Pilihan & validasi", "Choices & validation"],
        body: [
            "Opsi yang tidak kompatibel dinonaktifkan dengan alasan di tooltip. Error tampil langsung di bawah field beserta saran perbaikan.",
            "Incompatible options are disabled with a reason in the tooltip. Errors show right under the field with a suggested fix.",
        ],
    },
    TourStep {
        target: "context-panel",
        screen: Screen::Wizard,
        title: ["Panel konteks", "Context panel"],
        body: [
            "Arahkan kursor ke opsi untuk melihat penjelasan, kelebihan/kekurangan, dan perkiraan resource cluster.",
            "Hover an option to see what it does, its trade-offs, and the estimated cluster footprint.",
        ],
    },
    TourStep {
        target: "nav-footer",
        screen: Screen::Wizard,
        title: ["Navigasi", "Navigation"],
        body: [
            "Kembali/Lanjut. Lanjut hanya diblok oleh error. Review menampilkan semua file sebelum deploy.",
            "Back/Next. Only errors block Next. Review shows every generated file before deploy.",
        ],
    },
    TourStep {
        target: "help-button",
        screen: Screen::Wizard,
        title: ["Bantuan kontekstual", "Contextual help"],
        body: [
            "Tombol ⓘ di samping field membuka halaman dokumentasi yang relevan.",
            "The ⓘ button next to a field opens the relevant documentation page.",
        ],
    },
    TourStep {
        target: "docs-search",
        screen: Screen::Docs,
        title: ["Dokumentasi", "Documentation"],
        body: [
            "Cari konsep, referensi field, dan troubleshooting. Tur bisa diulang dari tombol Tur di header.",
            "Search concepts, field reference and troubleshooting. Replay the tour from the Tour button in the header.",
        ],
    },
];
