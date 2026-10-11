struct Skill {
    name: &'static str,
    triggers: &'static [&'static str],
    instructions: &'static str,
}

const SKILLS: &[Skill] = &[
    Skill {
        name: "analisis kritis",
        triggers: &["analisis", "bandingkan", "evaluasi", "risiko", "dampak", "kelebihan", "kekurangan"],
        instructions: "Uji asumsi utama, tunjukkan bukti dan risiko, lalu simpulkan secara seimbang.",
    },
    Skill {
        name: "pemecahan masalah",
        triggers: &["masalah", "solusi", "perbaiki", "bug", "error", "cara", "langkah"],
        instructions: "Urai masalah, pilih langkah praktis berurutan, dan sebutkan batasan penting.",
    },
    Skill {
        name: "riset web",
        triggers: &["terbaru", "terkini", "sekarang", "hari ini", "berita", "sumber", "riset", "internet"],
        instructions: "Utamakan bahan web yang disediakan, atribusikan sumber, dan bedakan fakta dari dugaan.",
    },
    Skill {
        name: "pemrograman",
        triggers: &["kode", "program", "rust", "python", "javascript", "implementasi", "fungsi"],
        instructions: "Berikan solusi teknis yang bisa diterapkan; jelaskan asumsi dan uji kasus penting.",
    },
    Skill {
        name: "ide kreatif",
        triggers: &["ide", "kreatif", "cerita", "desain", "meme", "caption"],
        instructions: "Tawarkan gagasan segar yang tetap relevan; emoji boleh dipakai secukupnya.",
    },
    Skill {
        name: "peringkasan",
        triggers: &["ringkas", "rangkuman", "simpulkan", "kesimpulan", "intinya"],
        instructions: "Dahulukan inti, lalu paling banyak dua detail yang benar-benar diperlukan.",
    },
];

pub fn select(topic: &str, limit: usize) -> Vec<&'static str> {
    let topic = topic.to_lowercase();
    let mut matches: Vec<_> = SKILLS
        .iter()
        .filter_map(|skill| {
            let score = skill
                .triggers
                .iter()
                .filter(|trigger| topic.contains(**trigger))
                .count();
            (score > 0).then_some((score, skill))
        })
        .collect();
    matches.sort_by(|(a, sa), (b, sb)| b.cmp(a).then_with(|| sa.name.cmp(sb.name)));
    matches
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill.instructions)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatically_selects_relevant_skills_with_a_cap() {
        let selected = select("Analisis risiko dan perbaiki bug Rust, lalu ringkas", 2);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|s| s.contains("asumsi")));
        assert!(selected.iter().any(|s| s.contains("langkah praktis")));
    }

    #[test]
    fn unrelated_topic_does_not_add_skill_prompt() {
        assert!(select("Halo, apa kabar?", 2).is_empty());
    }
}
