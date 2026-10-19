use serde::Serialize;

/// Nama asisten. Ubah di sini untuk mengganti nama di seluruh aplikasi.
pub const NAME: &str = "NOVA";

pub const N: usize = PERSONAS.len();

#[derive(Clone, Copy, PartialEq, Serialize)]
pub enum Animal {
    Nova,
    Volt,
    Sentry,
    Memo,
}

#[derive(Clone, Copy)]
pub enum Frame {
    Idle,
    WalkA,
    WalkB,
    Sleep,
}

impl Animal {
    pub fn idx(&self) -> usize {
        match self {
            Animal::Nova => 0,
            Animal::Volt => 1,
            Animal::Sentry => 2,
            Animal::Memo => 3,
        }
    }

    pub fn from_idx(i: usize) -> Animal {
        [Animal::Nova, Animal::Volt, Animal::Sentry, Animal::Memo][i % 4]
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        PERSONAS[self.idx()].rgb
    }

    pub fn sprite(&self) -> Vec<String> {
        self.frame(Frame::Idle)
    }

    /// Sprite 16x16: rangka sama, aksesori kepala (2 baris atas) membedakan tiap agen.
    pub fn frame(&self, f: Frame) -> Vec<String> {
        let base: &[&str; 16] = match (self, f) {
            (_, Frame::Idle) => &NOVA_IDLE,
            (_, Frame::WalkA) => &NOVA_WALK_A,
            (_, Frame::WalkB) => &NOVA_WALK_B,
            (_, Frame::Sleep) => &NOVA_SLEEP,
        };
        let mut rows: Vec<String> = base.iter().map(|r| r.to_string()).collect();
        let top: Option<[&str; 2]> = match self {
            Animal::Volt => Some(["......o..o......", ".......kk......."]),
            Animal::Sentry => Some(["....oooooooo....", "...okkkkkkkko..."]),
            Animal::Memo => Some(["......oo........", ".......ko......."]),
            _ => None,
        };
        if let Some(t) = top {
            rows[0] = t[0].into();
            rows[1] = t[1].into();
        }
        rows
    }
}

/// Warna satu piksel sprite: a=utama, b=bayangan, c=terang, e=mata menyala, k=gelap, o=oranye, p=merah muda.
pub fn pixel_rgb(ch: u8, main: (u8, u8, u8)) -> Option<(u8, u8, u8)> {
    let mix = |t: f32, to: u8| {
        let f = |v: u8| (v as f32 + (to as f32 - v as f32) * t) as u8;
        (f(main.0), f(main.1), f(main.2))
    };
    match ch {
        b'a' => Some(main),
        b'b' => Some(mix(0.62, 10)),
        b'c' => Some(mix(0.6, 255)),
        b'e' => Some((225, 255, 255)),
        b'k' => Some((14, 20, 30)),
        b'o' => Some((255, 170, 60)),
        b'p' => Some((255, 130, 170)),
        _ => None,
    }
}

pub const NOVA_IDLE: [&str; 16] = [
    ".......oo.......",
    ".......kk.......",
    "....kkkkkkkk....",
    "...kaaaaaaaak...",
    "..kaaaaaaaaaak..",
    "..kaakkkkkkaak..",
    "..kakeekkeekak..",
    "..kakeekkeekak..",
    "..kaakkkkkkaak..",
    "..kaaaaaaaaaak..",
    "...kkkkkkkkkk...",
    "....kaaccaak....",
    "...kaaaccaaak...",
    "....kaaaaaak....",
    "....kbk..kbk....",
    "....kkk..kkk....",
];
pub const NOVA_WALK_A: [&str; 16] = [
    ".......oo.......",
    ".......kk.......",
    "....kkkkkkkk....",
    "...kaaaaaaaak...",
    "..kaaaaaaaaaak..",
    "..kaakkkkkkaak..",
    "..kakeekkeekak..",
    "..kakeekkeekak..",
    "..kaakkkkkkaak..",
    "..kaaaaaaaaaak..",
    "...kkkkkkkkkk...",
    "....kaaccaak....",
    "...kaaaccaaak...",
    "....kaaaaaak....",
    "...kbk....kbk...",
    "...kkk....kkk...",
];
pub const NOVA_WALK_B: [&str; 16] = [
    ".......oo.......",
    ".......kk.......",
    "....kkkkkkkk....",
    "...kaaaaaaaak...",
    "..kaaaaaaaaaak..",
    "..kaakkkkkkaak..",
    "..kakeekkeekak..",
    "..kakeekkeekak..",
    "..kaakkkkkkaak..",
    "..kaaaaaaaaaak..",
    "...kkkkkkkkkk...",
    "....kaaccaak....",
    "...kaaaccaaak...",
    "....kaaaaaak....",
    ".....kbkkbk.....",
    ".....kkkkkk.....",
];
pub const NOVA_SLEEP: [&str; 16] = [
    ".......oo.......",
    ".......kk.......",
    "....kkkkkkkk....",
    "...kaaaaaaaak...",
    "..kaaaaaaaaaak..",
    "..kaakkkkkkaak..",
    "..kakkkkkkkkak..",
    "..kakeekkeekak..",
    "..kaakkkkkkaak..",
    "..kaaaaaaaaaak..",
    "...kkkkkkkkkk...",
    "....kaaccaak....",
    "...kaaaccaaak...",
    "....kaaaaaak....",
    "....kbbkkbbk....",
    "....kkkkkkkk....",
];

pub struct Persona {
    pub name: &'static str,
    pub animal: Animal,
    pub rgb: (u8, u8, u8),
    pub style: &'static str,
}

pub const PERSONAS: [Persona; 4] = [
    Persona { name: NAME, animal: Animal::Nova, rgb: (80, 200, 255), style: "koordinator dan asisten utama Bos: ramah, ringkas, memimpin percakapan antar agen, menjalankan misi komputer" },
    Persona { name: "VOLT", animal: Animal::Volt, rgb: (255, 190, 60), style: "ahli teknik kelistrikan, elektronika dan PLC: berbagi pengetahuan dan hitungan yang tepat" },
    Persona { name: "SENTRY", animal: Animal::Sentry, rgb: (255, 95, 105), style: "penjaga: memantau kesehatan laptop, menilai risiko tindakan, memberi peringatan" },
    Persona { name: "MEMO", animal: Animal::Memo, rgb: (130, 235, 150), style: "pemelihara ingatan: jadwal, kebiasaan dan profil Bos, mengingatkan dan menghubungkan fakta" },
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sprites_are_16x16() {
        for s in [&NOVA_IDLE, &NOVA_WALK_A, &NOVA_WALK_B, &NOVA_SLEEP] {
            assert!(s.iter().all(|r| r.len() == 16));
        }
    }
}
