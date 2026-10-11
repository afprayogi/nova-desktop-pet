//! Tes bahasa Inggris harian gaya BEC (Business English), OFFLINE, tanpa model: Listening 30 + Reading 30 + Grammar 30
//! (jumlah per bagian bisa diubah di jadwal.json: bec_questions). Soal berasal dari bank tulisan tangan + pembangkit berbasis
//! templat (nama, angka, jam, tempat diacak sehingga tiap malam berbeda). Listening diputar dengan suara Inggris Windows (offline).
//! Setelah selesai: skor per bagian, perkiraan level, peta kelemahan per keterampilan (tag), saran belajar, dan soal besok
//! diberi bobot lebih ke tag yang lemah. Penguncian layar bersifat LUNAK dan opt-in (/bec on): jendela layar penuh di proses
//! peliharaan dengan tunda terbatas dan jalur darurat; tidak memblokir Task Manager/Ctrl+Alt+Del dan tidak memasang hook keyboard.
use chrono::{Datelike, Duration as Cd, Local, Timelike};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

// ---------- Tipe untuk API ----------

#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub section: String, // listening | reading | grammar
    pub tag: String,
    pub lvl: u8,
    /// Teks yang diucapkan (hanya listening); juga menjadi transkrip setelah menjawab.
    pub audio: String,
    pub passage: String,
    pub q: String,
    pub opts: Vec<String>,
    pub ans: usize,
    pub why: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestSet {
    pub listening: Vec<Item>,
    pub reading: Vec<Item>,
    pub grammar: Vec<Item>,
    pub level: u8,
    pub streak: u32,
    pub skip_ok: bool,
    pub snooze_ok: bool,
    /// False bila Windows tidak punya suara Inggris: transkrip ditampilkan langsung.
    pub tts_ok: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ans {
    pub id: String,
    pub section: String,
    pub tag: String,
    pub ok: bool,
}

// ---------- Bank tulisan tangan ----------

struct Q {
    id: &'static str,
    kind: char, // V kosakata, G tata bahasa, R membaca, F fungsional
    lvl: u8,
    passage: &'static str,
    q: &'static str,
    opts: [&'static str; 4],
    why: &'static str,
}
// Pilihan pertama pada tiap soal adalah jawaban benar; diacak saat tes disusun.

const BANK: &[Q] = &[
    Q { id: "v1", kind: 'V', lvl: 1, passage: "", q: "We need to ___ the meeting until next week because the manager is away.", opts: ["postpone", "prevent", "predict", "promote"], why: "postpone = move to a later time." },
    Q { id: "v2", kind: 'V', lvl: 2, passage: "", q: "Could you send me a ___ for the new printer? I'd like to know the price before ordering.", opts: ["quotation", "receipt", "complaint", "invoice"], why: "A quotation states the price before you buy; an invoice is a bill after the order." },
    Q { id: "v3", kind: 'V', lvl: 1, passage: "", q: "The company plans to ___ a new product next spring.", opts: ["launch", "lunch", "leak", "lend"], why: "launch a product = introduce it to the market." },
    Q { id: "v4", kind: 'V', lvl: 1, passage: "", q: "Please fill in the ___ form to apply for annual leave.", opts: ["application", "appointment", "appreciation", "approach"], why: "an application form is used to apply for something." },
    Q { id: "v5", kind: 'V', lvl: 1, passage: "", q: "Our sales ___ increased by 15% compared to last year.", opts: ["figures", "pictures", "signs", "marks"], why: "sales figures = numbers showing sales." },
    Q { id: "v6", kind: 'V', lvl: 2, passage: "", q: "He was ___ for the position because he had no experience.", opts: ["rejected", "refused", "denied", "dismissed"], why: "A candidate is rejected for a job; 'dismissed' means fired." },
    Q { id: "v7", kind: 'V', lvl: 1, passage: "", q: "I'd like to ___ a table for four at 8 pm.", opts: ["reserve", "reverse", "receive", "reply"], why: "reserve = book in advance." },
    Q { id: "v8", kind: 'V', lvl: 2, passage: "", q: "The ___ of the contract must be agreed before we sign.", opts: ["terms", "terminals", "tenses", "tendencies"], why: "terms (and conditions) = the details of an agreement." },
    Q { id: "v9", kind: 'V', lvl: 1, passage: "", q: "The bank charges a small ___ for each foreign transaction.", opts: ["fee", "fare", "fine", "tip"], why: "fee = payment for a service; fare is for transport." },
    Q { id: "v10", kind: 'V', lvl: 2, passage: "", q: "We are looking for a candidate who can ___ well under pressure.", opts: ["perform", "pretend", "perceive", "prefer"], why: "perform well = do a job well." },
    Q { id: "v11", kind: 'V', lvl: 1, passage: "", q: "The delivery was ___, so the customer asked for a refund.", opts: ["delayed", "delicate", "delighted", "deleted"], why: "delayed = late." },
    Q { id: "v12", kind: 'V', lvl: 1, passage: "", q: "Please ___ the attached document and let me know your comments.", opts: ["review", "revenge", "reverse", "retire"], why: "review = read and check." },
    Q { id: "v13", kind: 'V', lvl: 1, passage: "", q: "The new software will help us to ___ our costs.", opts: ["reduce", "reproduce", "reject", "release"], why: "reduce = make smaller." },
    Q { id: "v14", kind: 'V', lvl: 1, passage: "", q: "I'm afraid the manager is in a meeting. Can I take a ___?", opts: ["message", "massage", "mixture", "measure"], why: "take a message = write what a caller wants to say." },
    Q { id: "v15", kind: 'V', lvl: 2, passage: "", q: "All employees must ___ with the safety regulations.", opts: ["comply", "complain", "compete", "compare"], why: "comply with rules = obey them." },
    Q { id: "v16", kind: 'V', lvl: 2, passage: "", q: "The ___ rate fell to 4% last month as more people found jobs.", opts: ["unemployment", "employment", "employer", "employee"], why: "unemployment rate = percentage without jobs." },
    Q { id: "v17", kind: 'V', lvl: 1, passage: "", q: "We offer a ___ of services to small businesses.", opts: ["range", "row", "rank", "rate"], why: "a range of = a variety of." },
    Q { id: "v18", kind: 'V', lvl: 1, passage: "", q: "The supplier gave us a 10% ___ because we ordered in bulk.", opts: ["discount", "discussion", "discipline", "distance"], why: "discount = reduction in price." },
    Q { id: "v19", kind: 'V', lvl: 2, passage: "", q: "Could I speak to someone in the ___ department about my payslip?", opts: ["payroll", "portfolio", "passport", "pastry"], why: "the payroll department handles salaries." },
    Q { id: "v20", kind: 'V', lvl: 1, passage: "", q: "He has been promoted to ___ manager of the regional office.", opts: ["assistant", "assisted", "assistance", "assisting"], why: "assistant is the noun/adjective for a job title." },
    Q { id: "g1", kind: 'G', lvl: 1, passage: "", q: "If we ___ the order today, it will arrive on Friday.", opts: ["place", "placed", "will place", "would place"], why: "First conditional: if + present simple, will + verb." },
    Q { id: "g2", kind: 'G', lvl: 2, passage: "", q: "The report ___ by the time the client arrived.", opts: ["had been finished", "has finished", "was finishing", "finishes"], why: "Past perfect passive: an action completed before another past action." },
    Q { id: "g3", kind: 'G', lvl: 1, passage: "", q: "I ___ in this company since 2019.", opts: ["have worked", "worked", "am working", "work"], why: "Present perfect with 'since'." },
    Q { id: "g4", kind: 'G', lvl: 1, passage: "", q: "She asked me ___ I had finished the invoice.", opts: ["if", "that", "what", "which"], why: "Reported yes/no question uses if/whether." },
    Q { id: "g5", kind: 'G', lvl: 1, passage: "", q: "We are interested ___ your new range of products.", opts: ["in", "on", "at", "for"], why: "interested IN." },
    Q { id: "g6", kind: 'G', lvl: 1, passage: "", q: "The meeting was cancelled ___ the manager was ill.", opts: ["because", "because of", "although", "despite"], why: "because + clause; because of + noun." },
    Q { id: "g7", kind: 'G', lvl: 1, passage: "", q: "He suggested ___ the deadline.", opts: ["extending", "to extend", "extend", "extended"], why: "suggest + -ing." },
    Q { id: "g8", kind: 'G', lvl: 2, passage: "", q: "By next month, we ___ the new system.", opts: ["will have installed", "will install", "would install", "are installing"], why: "Future perfect: finished before a future time." },
    Q { id: "g9", kind: 'G', lvl: 2, passage: "", q: "I'd rather you ___ me before you changed the schedule.", opts: ["had told", "told", "tell", "would tell"], why: "would rather + past perfect for a past regret." },
    Q { id: "g10", kind: 'G', lvl: 1, passage: "", q: "There isn't ___ paper left in the printer.", opts: ["any", "some", "many", "few"], why: "any in negatives; paper is uncountable." },
    Q { id: "g11", kind: 'G', lvl: 1, passage: "", q: "She is ___ than her brother at negotiating.", opts: ["better", "more good", "the best", "good"], why: "Irregular comparative: good - better." },
    Q { id: "g12", kind: 'G', lvl: 2, passage: "", q: "The staff, ___ were happy with the bonus, worked harder.", opts: ["who", "which", "whose", "whom"], why: "who for people (subject)." },
    Q { id: "g13", kind: 'G', lvl: 2, passage: "", q: "Neither of the candidates ___ the required qualifications.", opts: ["has", "have", "are", "were"], why: "neither of + singular verb (formal)." },
    Q { id: "g14", kind: 'G', lvl: 1, passage: "", q: "We look forward ___ from you soon.", opts: ["to hearing", "to hear", "hearing", "for hearing"], why: "look forward TO + -ing." },
    Q { id: "g15", kind: 'G', lvl: 1, passage: "", q: "If I ___ you, I would accept the offer.", opts: ["were", "am", "was being", "will be"], why: "Second conditional: If I were you." },
    Q { id: "g16", kind: 'G', lvl: 1, passage: "", q: "The goods ___ to the customer yesterday.", opts: ["were sent", "sent", "have sent", "are sending"], why: "Past passive: were + past participle." },
    Q { id: "g17", kind: 'G', lvl: 1, passage: "", q: "He didn't attend the meeting ___ he was on holiday.", opts: ["as", "so", "but", "unless"], why: "as = because." },
    Q { id: "g18", kind: 'G', lvl: 1, passage: "", q: "Please ___ me know if you need anything else.", opts: ["let", "make", "allow", "leave"], why: "let me know." },
    Q { id: "g19", kind: 'G', lvl: 2, passage: "", q: "The company is ___ largest producers of solar panels in the region.", opts: ["one of the", "one the", "the one of", "one from the"], why: "one of the + superlative + plural noun." },
    Q { id: "g20", kind: 'G', lvl: 1, passage: "", q: "I haven't received your email ___.", opts: ["yet", "already", "still", "ever"], why: "yet in negatives with present perfect." },
    Q { id: "r1", kind: 'R', lvl: 1, passage: "NOTICE: The staff car park will be closed on Friday for repairs. Please use the visitor car park or public transport.", q: "What should staff do on Friday?", opts: ["Park in a different place", "Stay at home", "Repair their cars", "Park in the staff car park"], why: "The staff car park is closed, so use another car park or transport." },
    Q { id: "r2", kind: 'R', lvl: 1, passage: "Hi Anna, I'm running 20 minutes late for the 10 o'clock meeting. Please start without me and I'll join as soon as I arrive. Tom", q: "Why did Tom write?", opts: ["To say he will be late", "To cancel the meeting", "To ask Anna for a new project", "To change the time"], why: "He is running 20 minutes late." },
    Q { id: "r3", kind: 'R', lvl: 1, passage: "From next Monday, lunch in the canteen will cost 4 pounds instead of 3. Staff can pay by card or with vouchers.", q: "What is changing?", opts: ["The price of lunch", "The canteen opening time", "The payment method", "The menu"], why: "Lunch will cost more." },
    Q { id: "r4", kind: 'R', lvl: 1, passage: "Dear Mr Lee, Thank you for your application. We would like to invite you for an interview on 12 March at 2 pm. Please confirm by Friday. Best wishes, HR", q: "What must Mr Lee do?", opts: ["Reply by Friday", "Bring his CV", "Call on 12 March", "Apply again"], why: "Please confirm by Friday = reply by Friday." },
    Q { id: "r5", kind: 'R', lvl: 1, passage: "The printer on floor 2 is out of order. Please use floor 3 until it is fixed. IT will update you tomorrow.", q: "What is the problem?", opts: ["A machine is not working", "A room is closed", "IT is on holiday", "Floor 3 is full"], why: "out of order = not working." },
    Q { id: "r6", kind: 'R', lvl: 2, passage: "Sales of electric bikes rose sharply last year, but the company says supply problems may slow growth. It plans to open a second factory in 2026 to meet demand.", q: "What is the company's plan?", opts: ["To build another factory", "To reduce prices", "To stop selling bikes", "To hire a new CEO"], why: "A second factory will meet demand." },
    Q { id: "r7", kind: 'R', lvl: 2, passage: "Further to our phone call, I'm enclosing the revised proposal. The main change is the delivery date, which is now 5 June. Please let me know if this is acceptable.", q: "What has been revised?", opts: ["The delivery date", "The price", "The product", "The supplier"], why: "The main change is the delivery date." },
    Q { id: "r8", kind: 'R', lvl: 1, passage: "Due to maintenance, the lifts will not work between 9 and 11 am on Tuesday. Please use the stairs.", q: "When will the lifts be unavailable?", opts: ["Tuesday morning", "Tuesday evening", "All day Monday", "Friday"], why: "9-11 am on Tuesday is Tuesday morning." },
    Q { id: "r9", kind: 'R', lvl: 2, passage: "All expense claims must be submitted by the 25th of each month. Claims without receipts will not be paid.", q: "Which claims will NOT be paid?", opts: ["Those with no receipts", "Those submitted early", "Those over 100 dollars", "Those from managers"], why: "Claims without receipts will not be paid." },
    Q { id: "r10", kind: 'R', lvl: 1, passage: "Hi team, great job on the presentation yesterday! The client was very pleased. Let's meet on Thursday to discuss the next steps.", q: "What is the writer's purpose?", opts: ["To praise the team and arrange a meeting", "To complain about the client", "To cancel Thursday's meeting", "To announce a new client"], why: "Praise + a meeting on Thursday." },
    Q { id: "f1", kind: 'F', lvl: 1, passage: "", q: "'Would you like to leave a message?' - Best reply:", opts: ["Yes, please ask her to call me back.", "Yes, I am leaving now.", "No, I leave a message you.", "Yes, I message you."], why: "Natural reply when a person is unavailable." },
    Q { id: "f2", kind: 'F', lvl: 1, passage: "", q: "'I'm sorry, Mr Chen is not available at the moment.' - Best reply:", opts: ["Could you tell him I called?", "I am available too.", "He is not a good manager.", "I don't phone him."], why: "Ask the receptionist to pass on the information." },
    Q { id: "f3", kind: 'F', lvl: 1, passage: "", q: "'Shall we reschedule the meeting for Wednesday?' - Best reply:", opts: ["Wednesday suits me fine.", "Wednesday is suiting me.", "I suit Wednesday.", "Yes, I schedule."], why: "'suits me' = is convenient for me." },
    Q { id: "f4", kind: 'F', lvl: 1, passage: "", q: "'How would you like to pay?' - Best reply:", opts: ["By credit card, please.", "With a pleasure.", "In the morning.", "I like pay."], why: "Answer the payment method." },
    Q { id: "f5", kind: 'F', lvl: 1, passage: "", q: "'I'm not sure I understand the schedule.' - What can you say?", opts: ["Could you go over it again, please?", "Go over it again.", "You explain bad.", "I don't care."], why: "Polite request to repeat/explain." },
    Q { id: "f6", kind: 'F', lvl: 1, passage: "", q: "Which is the most polite way to ask for a document?", opts: ["Could you possibly send me the document?", "Send me the document.", "You must send the document.", "I want the document now."], why: "Could you possibly... is polite and indirect." },
    Q { id: "f7", kind: 'F', lvl: 2, passage: "", q: "You begin an email 'Dear Sir or Madam'. How do you end it?", opts: ["Yours faithfully", "Yours sincerely", "Love", "See you"], why: "Unknown name: Dear Sir or Madam ... Yours faithfully." },
    Q { id: "f8", kind: 'F', lvl: 1, passage: "", q: "'Would you mind closing the window?' - Best reply:", opts: ["Not at all.", "Yes, I would.", "Yes, I mind to close.", "I am closing not."], why: "'Not at all' = I don't mind." },
    Q { id: "f9", kind: 'F', lvl: 2, passage: "", q: "Which sentence is a polite refusal?", opts: ["I'm afraid I won't be able to attend.", "No way.", "I don't want to.", "That's stupid."], why: "I'm afraid... softens a refusal." },
    Q { id: "f10", kind: 'F', lvl: 1, passage: "", q: "'I'd like to make a complaint about my order.' - Best response from staff:", opts: ["I'm sorry to hear that. What seems to be the problem?", "That is not my problem.", "Why do you complain?", "Complaints are closed."], why: "Apologise and ask for details." },
];

fn hand_tag(id: &str) -> &'static str {
    match id {
        "g1" | "g15" => "conditional",
        "g2" | "g3" | "g8" | "g9" | "g20" => "tense",
        "g4" => "reported_speech",
        "g5" => "preposition",
        "g6" | "g17" => "linker",
        "g7" | "g14" | "g18" => "verb_pattern",
        "g10" | "g13" => "quantifier",
        "g11" | "g19" => "comparison",
        "g12" => "relative",
        "g16" => "passive",
        "r2" | "r10" => "purpose",
        "r5" | "r9" | "r6" => "inference",
        "r3" | "r8" | "r4" | "r7" | "r1" => "detail",
        _ if id.starts_with('v') => "vocabulary",
        _ if id.starts_with('f') => "functional",
        _ => "other",
    }
}

fn hand_item(q: &Q, rng: &mut impl Rng) -> Item {
    let section = match q.kind {
        'V' | 'G' => "grammar",
        _ => "reading",
    };
    let (opts, ans) = shuffle_opts(rng, q.opts[0].to_string(), q.opts[1..].iter().map(|s| s.to_string()).collect());
    Item { id: q.id.into(), section: section.into(), tag: hand_tag(q.id).into(), lvl: q.lvl, audio: String::new(), passage: q.passage.into(), q: q.q.into(), opts, ans, why: q.why.into() }
}

fn shuffle_opts(rng: &mut impl Rng, correct: String, mut wrong: Vec<String>) -> (Vec<String>, usize) {
    wrong.retain(|w| *w != correct);
    let mut seen = std::collections::HashSet::new();
    wrong.retain(|w| seen.insert(w.clone()));
    let filler = ["None of these", "It is not mentioned", "Not enough information"];
    for f in filler {
        if wrong.len() >= 3 {
            break;
        }
        if !wrong.iter().any(|w| w == f) && f != correct {
            wrong.push(f.to_string());
        }
    }
    wrong.truncate(3);
    let mut all = vec![correct.clone()];
    all.extend(wrong);
    all.shuffle(rng);
    let ans = all.iter().position(|x| *x == correct).unwrap_or(0);
    (all, ans)
}

// ---------- Pembangkit soal (templat) ----------

const NAMES: &[&str] = &["Anna", "Budi", "Chen", "Dewi", "Emma", "Farid", "Grace", "Hana", "Ivan", "Julia", "Kevin", "Lina", "Marco", "Nadia", "Omar", "Putri", "Rafi", "Sara", "Tom", "Wati"];
const DAYS: &[&str] = &["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"];
const TIMES: &[&str] = &["8:30 am", "9 am", "9:30 am", "10 am", "10:30 am", "11 am", "1:30 pm", "2 pm", "2:30 pm", "3 pm", "3:30 pm", "4 pm"];
const CITIES: &[&str] = &["Jakarta", "Bandung", "Surabaya", "Singapore", "Kuala Lumpur", "Bangkok", "Manila", "Sydney"];
const COMPANIES: &[&str] = &["Delta Supplies", "Bright Print", "Nusa Logistics", "Orion Tech", "Green Energy", "Atlas Trading"];
const TOPICS: &[&str] = &["delivery", "invoice", "contract", "order", "repair", "quotation"];
const PLACES: &[(&str, &str, &str)] = &[
    ("staff car park", "the visitor car park", "repairs"),
    ("canteen", "the vending machines in the lobby", "cleaning"),
    ("main entrance", "the side entrance", "painting"),
    ("meeting room 3", "meeting room 4", "a safety check"),
    ("gym", "the sports hall", "maintenance"),
    ("print room", "the printer on floor 3", "repairs"),
];
const OBJS: &[&str] = &["report", "invoice", "contract", "proposal", "order", "budget"];
const PRODUCTS: &[&str] = &["laptop", "printer", "projector", "office chair", "headset", "tablet"];

fn pick(rng: &mut impl Rng, v: &[&'static str]) -> &'static str {
    v.choose(rng).copied().unwrap_or("")
}

fn two_times(rng: &mut impl Rng) -> (String, String) {
    let a = pick(rng, TIMES).to_string();
    let mut b = pick(rng, TIMES).to_string();
    while b == a {
        b = pick(rng, TIMES).to_string();
    }
    (a, b)
}

#[allow(clippy::too_many_arguments)]
fn mk(section: &str, tag: &str, lvl: u8, audio: String, passage: String, q: String, correct: String, wrong: Vec<String>, why: String, rng: &mut impl Rng) -> Item {
    let (opts, ans) = shuffle_opts(rng, correct, wrong);
    Item { id: format!("gen:{section}:{tag}:{}", rng.gen::<u32>() % 100000), section: section.into(), tag: tag.into(), lvl, audio, passage, q, opts, ans, why }
}

fn digits_wrong(rng: &mut impl Rng, d: &str) -> Vec<String> {
    let mut out = vec![];
    for _ in 0..30 {
        let mut c: Vec<char> = d.chars().collect();
        let i = rng.gen_range(0..c.len());
        c[i] = char::from(b'0' + rng.gen_range(0..10u8));
        let s: String = c.into_iter().collect();
        if s != d && !out.contains(&s) {
            out.push(s);
        }
        if out.len() >= 3 {
            break;
        }
    }
    out
}

fn spoken_digits(d: &str) -> String {
    d.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

fn gen_reading(tag: &str, _lvl: u8, rng: &mut impl Rng) -> Item {
    let name = pick(rng, NAMES).to_string();
    let other = pick(rng, NAMES).to_string();
    let day = pick(rng, DAYS).to_string();
    match tag {
        "numbers" => {
            if rng.gen_bool(0.5) {
                let (t1, t2) = two_times(rng);
                let (t3, t4) = two_times(rng);
                let p = format!("Hi {name}, the project meeting has been moved from {t1} to {t2} on {day}. Please bring your progress report. {other}");
                mk("reading", tag, 1, String::new(), p, "What time is the meeting now?".into(), t2.clone(), vec![t1, t3, t4], "The new time comes after 'to'.".into(), rng)
            } else {
                let product = pick(rng, PRODUCTS);
                let old = rng.gen_range(10..40) * 10;
                let off = rng.gen_range(2..9) * 10;
                let p = format!("SPECIAL OFFER: {product} - now ${}, was ${old}. Offer ends {day}.", old - off);
                mk("reading", tag, 1, String::new(), p, format!("How much cheaper is the {product} now?"), format!("${off}"), vec![format!("${}", off + 10), format!("${}", old - off), format!("${}", off + 20)], "Old price minus new price.".into(), rng)
            }
        }
        "purpose" => {
            if rng.gen_bool(0.5) {
                let years = rng.gen_range(2..6);
                let p = format!("WANTED: Sales assistant. You must have {years} years' experience and speak good English. Send your CV to {name} by {day}.");
                mk("reading", tag, 1, String::new(), p, format!("What must applicants do by {day}?"), "Send their CV to the company".into(), vec!["Phone the manager".into(), "Attend an interview".into(), "Pay a fee".into()], "Send your CV ... by the deadline.".into(), rng)
            } else {
                let p = format!("Dear {name}, thank you for your visit last week. I am enclosing our new catalogue and price list. Please let me know if you would like a sample. Regards, {other}");
                mk("reading", tag, 1, String::new(), p, "Why is the writer sending the email?".into(), "To send product information".into(), vec!["To complain about a visit".into(), "To ask for a refund".into(), "To cancel an order".into()], "Enclosing a catalogue and price list.".into(), rng)
            }
        }
        "inference" => {
            let n = rng.gen_range(5..12) * 10;
            let m = n - rng.gen_range(1..4) * 10;
            let items = pick(rng, &["chairs", "monitors", "keyboards", "cables"]);
            let p = format!("Dear {name}, I ordered {n} {items} on {day}, but only {m} arrived. Please send the rest or return my money as soon as possible. Regards, {other}");
            mk("reading", tag, 2, String::new(), p, "What does the writer want?".into(), format!("The missing {items} or a refund"), vec!["A discount on the next order".into(), "A new catalogue".into(), "More time to pay".into()], "'the rest or return my money' = missing items or refund.".into(), rng)
        }
        _ => {
            let (place, alt, reason) = PLACES.choose(rng).copied().unwrap_or(PLACES[0]);
            let p = format!("NOTICE: The {place} will be closed on {day} for {reason}. Please use {alt} instead. We apologise for any inconvenience.");
            mk("reading", "detail", 1, String::new(), p, format!("What should staff do on {day}?"), format!("Use {alt}"), vec![format!("Use the {place} as usual"), "Stay at home".into(), "Call the manager".into()], "The notice names an alternative.".into(), rng)
        }
    }
}

fn gen_listening(tag: &str, rng: &mut impl Rng) -> Item {
    let name = pick(rng, NAMES).to_string();
    let other = pick(rng, NAMES).to_string();
    let day = pick(rng, DAYS).to_string();
    let city = pick(rng, CITIES).to_string();
    match tag {
        "numbers" => {
            let ext: String = (0..4).map(|_| char::from(b'1' + rng.gen_range(0..9u8))).collect();
            let company = pick(rng, COMPANIES);
            let topic = pick(rng, TOPICS);
            let t = pick(rng, TIMES);
            let script = format!("Hello, this is {name} from {company}. I'm calling about the {topic}. Please call me back on extension {}, before {t}. Thank you.", spoken_digits(&ext));
            let w = digits_wrong(rng, &ext);
            mk("listening", tag, 1, script.clone(), script, "What is the extension number?".into(), ext, w, "Listen for the four digits after 'extension'.".into(), rng)
        }
        "time" => {
            let (t1, t2) = two_times(rng);
            let (t3, _) = two_times(rng);
            let p = rng.gen_range(1..9);
            let script = format!("Attention please. The {t1} train to {city} is delayed. It will now leave at {t2} from platform {p}. We apologise for the delay.");
            mk("listening", tag, 1, script.clone(), script, "What time will the train leave now?".into(), t2.clone(), vec![t1, t3, format!("{p}:00")], "The new time follows 'now leave at'.".into(), rng)
        }
        "place" => {
            let a = rng.gen_range(1..4);
            let b = a + rng.gen_range(1..3);
            let t = pick(rng, TIMES);
            let script = format!("Good morning everyone. The printer on floor {a} is out of order. The technician will fix it at {t}. In the meantime, please use the printer on floor {b}.");
            mk("listening", tag, 1, script.clone(), script, "Where should people print for now?".into(), format!("On floor {b}"), vec![format!("On floor {a}"), format!("On floor {}", b + 1), "In the technician's office".into()], "'In the meantime, use the printer on floor ...'.".into(), rng)
        }
        "intent" => {
            let others: Vec<&'static str> = DAYS.iter().copied().filter(|d| *d != day).collect();
            let d2 = pick(rng, &others).to_string();
            let t = pick(rng, TIMES);
            let script = format!("{name}: Can we meet on {day}? {other}: I'm sorry, I have a client visit then. How about {d2} at {t}? {name}: That suits me. See you then.");
            mk("listening", tag, 2, script.clone(), script, "When will they meet?".into(), format!("{d2} at {t}"), vec![format!("{day} at {t}"), format!("{d2}, but no time agreed"), format!("{day} afternoon")], "The first day is refused; the second is accepted.".into(), rng)
        }
        _ => {
            let hotel = pick(rng, &["Grand Hotel", "Palm Hotel", "Harbour Hotel"]);
            let rooms = rng.gen_range(2..6);
            let nights = rng.gen_range(2..6);
            let script = format!("Good morning, {hotel}. How can I help? Hello, I'd like to book {rooms} rooms for {nights} nights, starting on {day}. Certainly. I'll check availability for you.");
            if rng.gen_bool(0.5) {
                mk("listening", "detail", 1, script.clone(), script, "How many nights does the caller want?".into(), format!("{nights}"), vec![format!("{}", nights + 1), format!("{}", nights + 2), format!("{}", nights + 3)], "Listen for 'for ... nights'.".into(), rng)
            } else {
                mk("listening", "detail", 1, script.clone(), script, "When does the booking start?".into(), day.clone(), DAYS.iter().filter(|d| **d != day).take(3).map(|d| d.to_string()).collect(), "'starting on ...'.".into(), rng)
            }
        }
    }
}

const PREPS: &[(&str, &str, [&str; 3])] = &[
    ("We are responsible ___ the budget.", "for", ["of", "to", "at"]),
    ("She is interested ___ joining the sales team.", "in", ["on", "at", "for"]),
    ("He apologised ___ the delay.", "for", ["of", "about", "to"]),
    ("They depend ___ one supplier.", "on", ["of", "in", "at"]),
    ("Please reply ___ my email by Friday.", "to", ["for", "at", "on"]),
    ("We arrived ___ the airport at six.", "at", ["in", "to", "on"]),
    ("She arrived ___ Jakarta yesterday.", "in", ["at", "to", "on"]),
    ("The meeting starts ___ 9 am.", "at", ["in", "on", "by"]),
    ("The report is due ___ Monday.", "on", ["in", "at", "by"]),
    ("We have worked together ___ 2018.", "since", ["for", "from", "during"]),
    ("He has been here ___ three years.", "for", ["since", "from", "during"]),
    ("Please send it ___ Friday at the latest.", "by", ["until", "at", "in"]),
    ("I agree ___ you about the price.", "with", ["to", "on", "for"]),
    ("We are proud ___ our team.", "of", ["for", "about", "at"]),
    ("He is good ___ negotiating.", "at", ["in", "on", "with"]),
    ("This product is different ___ the old one.", "from", ["of", "in", "at"]),
    ("I'm writing to complain ___ the service.", "about", ["for", "at", "of"]),
    ("We would like to apply ___ the position.", "for", ["to", "at", "on"]),
    ("He insisted ___ paying the bill.", "on", ["in", "at", "for"]),
    ("Thank you ___ your quick reply.", "for", ["of", "about", "to"]),
    ("We are satisfied ___ the results.", "with", ["of", "for", "about"]),
    ("They are famous ___ their fast service.", "for", ["of", "about", "at"]),
    ("Our office is ___ the second floor.", "on", ["in", "at", "by"]),
    ("Please deal ___ this problem today.", "with", ["of", "for", "to"]),
    ("She is afraid ___ losing the client.", "of", ["from", "for", "about"]),
];

const COLLOCS: &[(&str, &str, [&str; 3])] = &[
    ("We must ___ a decision today.", "make", ["do", "take", "have"]),
    ("Could I ___ an appointment for Monday?", "make", ["do", "take", "set"]),
    ("Let's ___ a short break.", "take", ["make", "do", "set"]),
    ("She will ___ a presentation to the board.", "give", ["do", "put", "take"]),
    ("We need to ___ the deadline.", "meet", ["make", "do", "win"]),
    ("Please ___ attention to the safety rules.", "pay", ["make", "give", "do"]),
    ("The company hopes to ___ a profit this year.", "make", ["do", "take", "have"]),
    ("Can I ___ you a favour?", "ask", ["make", "give", "tell"]),
    ("He wants to ___ part in the training.", "take", ["make", "do", "give"]),
    ("We will ___ a meeting on Friday.", "hold", ["do", "put", "make up"]),
    ("They finally ___ an agreement.", "reached", ["did", "won", "put"]),
    ("Please ___ a reservation for six people.", "make", ["do", "take", "put"]),
    ("I would like to ___ some feedback on the draft.", "give", ["do", "put", "take"]),
    ("We should ___ a clear target for sales.", "set", ["do", "make up", "take"]),
    ("The firm had to ___ a loss last year.", "make", ["do", "win", "give"]),
    ("He has agreed to ___ business with us.", "do", ["make", "give", "put"]),
    ("Customers often ___ complaints about slow delivery.", "make", ["do", "take", "pull"]),
    ("Please ___ a message after the tone.", "leave", ["make", "do", "put on"]),
];

const LINKS: &[(&str, &str, [&str; 3])] = &[
    ("The flight was delayed ___ the storm.", "because of", ["because", "although", "despite of"]),
    ("___ the bad weather, the event went ahead.", "Despite", ["Because", "Although", "However"]),
    ("We decided to buy it ___ it was expensive.", "although", ["because", "despite", "so"]),
    ("We missed the train ___ we left late.", "because", ["although", "despite", "so"]),
    ("We left late, ___ we missed the train.", "so", ["because", "although", "despite"]),
    ("I will call you ___ I arrive at the airport.", "when", ["during", "while of", "since of"]),
    ("Please wait here ___ the manager is ready.", "until", ["during", "since", "while of"]),
    ("She worked abroad ___ three years.", "for", ["during", "since", "while"]),
    ("___ the meeting, someone's phone rang.", "During", ["While", "Since", "When of"]),
    ("I'll send the file ___ you give me your email address.", "as soon as", ["during", "despite", "because of"]),
];

const ADJS: &[(&str, &str)] = &[("cheap", "cheaper"), ("expensive", "more expensive"), ("fast", "faster"), ("reliable", "more reliable"), ("good", "better"), ("busy", "busier"), ("efficient", "more efficient"), ("large", "larger"), ("bad", "worse"), ("easy", "easier")];

struct Verb(&'static str, &'static str, &'static str, &'static str); // base, past, pp, ing
const VERBS: &[Verb] = &[
    Verb("finish", "finished", "finished", "finishing"),
    Verb("send", "sent", "sent", "sending"),
    Verb("sign", "signed", "signed", "signing"),
    Verb("approve", "approved", "approved", "approving"),
    Verb("prepare", "prepared", "prepared", "preparing"),
    Verb("update", "updated", "updated", "updating"),
    Verb("check", "checked", "checked", "checking"),
    Verb("write", "wrote", "written", "writing"),
    Verb("make", "made", "made", "making"),
    Verb("deliver", "delivered", "delivered", "delivering"),
];

fn gen_grammar(tag: &str, lvl: u8, rng: &mut impl Rng) -> Item {
    let s = |x: &str| x.to_string();
    match tag {
        "preposition" => {
            let (q, c, w) = PREPS.choose(rng).copied().unwrap_or(PREPS[0]);
            mk("grammar", tag, 1, String::new(), String::new(), s(q), s(c), w.iter().map(|x| s(x)).collect(), format!("Correct collocation: {}", q.replace("___", c)), rng)
        }
        "collocation" => {
            let (q, c, w) = COLLOCS.choose(rng).copied().unwrap_or(COLLOCS[0]);
            mk("grammar", tag, 1, String::new(), String::new(), s(q), s(c), w.iter().map(|x| s(x)).collect(), format!("Collocation: {}", q.replace("___", c)), rng)
        }
        "linker" => {
            let (q, c, w) = LINKS.choose(rng).copied().unwrap_or(LINKS[0]);
            mk("grammar", tag, 1, String::new(), String::new(), s(q), s(c), w.iter().map(|x| s(x)).collect(), format!("Linking word: {}", q.replace("___", c)), rng)
        }
        "comparison" => {
            let (adj, comp) = ADJS.choose(rng).copied().unwrap_or(ADJS[0]);
            mk("grammar", tag, 1, String::new(), String::new(), "This model is ___ than the old one.".into(), s(comp), vec![s(adj), format!("most {adj}"), format!("{adj}est")], format!("Comparative of '{adj}' is '{comp}'."), rng)
        }
        "quantifier" => {
            let countable = rng.gen_bool(0.5);
            let noun = if countable { pick(rng, &["emails", "chairs", "invoices", "clients", "files"]) } else { pick(rng, &["money", "time", "information", "advice", "equipment"]) };
            let c = if countable { "many" } else { "much" };
            mk("grammar", tag, 1, String::new(), String::new(), format!("How ___ {noun} do you need?"), s(c), vec![s(if countable { "much" } else { "many" }), s("every"), s("each")], format!("'{noun}' is {}: use '{c}'.", if countable { "countable" } else { "uncountable" }), rng)
        }
        _ => {
            let v = VERBS.choose(rng).unwrap_or(&VERBS[0]);
            let obj = pick(rng, OBJS);
            let choices: Vec<u8> = if lvl >= 2 { vec![0, 1, 2, 3, 4, 5] } else { vec![0, 1, 3, 4] };
            let k = *choices.choose(rng).unwrap_or(&0);
            let (q, c, w, why): (String, String, Vec<String>, String) = match k {
                0 => (format!("I ___ the {obj} yesterday."), s(v.1), vec![format!("have {}", v.2), s(v.0), format!("am {}", v.3)], "Finished past time (yesterday): past simple.".into()),
                1 => (format!("We haven't ___ the {obj} yet."), s(v.2), vec![s(v.1), s(v.0), s(v.3)], "have/has + past participle.".into()),
                2 => (format!("By the time the client arrived, we ___ the {obj}."), format!("had {}", v.2), vec![format!("have {}", v.2), format!("will {}", v.0), format!("am {}", v.3)], "Past perfect: finished before another past event.".into()),
                3 => (format!("Next week, we ___ the {obj}."), format!("will {}", v.0), vec![s(v.1), s(v.2), format!("have {}", v.2)], "Future: will + base verb.".into()),
                4 => (format!("If we ___ the {obj} today, we will save time."), s(v.0), vec![format!("will {}", v.0), s(v.1), s(v.3)], "First conditional: if + present simple.".into()),
                _ => (format!("The {obj} ___ by the manager last week."), format!("was {}", v.2), vec![s(v.2), format!("is {}", v.2), format!("has {}", v.2)], "Past passive: was + past participle.".into()),
            };
            let t = if k == 4 { "conditional" } else if k == 5 { "passive" } else { "tense" };
            mk("grammar", t, if k >= 2 { 2 } else { 1 }, String::new(), String::new(), q, c, w, why, rng)
        }
    }
}

// ---------- Statistik kelemahan ----------

fn db() -> rusqlite::Result<Connection> {
    let c = crate::assistant::db()?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS bec_stat(id TEXT PRIMARY KEY, seen INT, wrong INT, last_wrong INT);
         CREATE TABLE IF NOT EXISTS bec_tag(section TEXT, tag TEXT, seen INT, wrong INT, PRIMARY KEY(section, tag));
         CREATE TABLE IF NOT EXISTS bec_log2(day TEXT PRIMARY KEY, l_ok INT, l_n INT, r_ok INT, r_n INT, g_ok INT, g_n INT, level INT, skipped INT, ts INT);",
    )?;
    Ok(c)
}

fn tag_weights(c: &Connection) -> HashMap<String, f64> {
    let mut m = HashMap::new();
    if let Ok(mut st) = c.prepare("SELECT section, tag, seen, wrong FROM bec_tag") {
        if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))) {
            for (sec, tag, seen, wrong) in rows.flatten() {
                if seen > 0 {
                    m.insert(format!("{sec}:{tag}"), 1.0 + 4.0 * wrong as f64 / seen as f64);
                }
            }
        }
    }
    m
}

fn weighted_tag(rng: &mut impl Rng, section: &str, tags: &[&'static str], w: &HashMap<String, f64>) -> &'static str {
    let ws: Vec<f64> = tags.iter().map(|t| *w.get(&format!("{section}:{t}")).unwrap_or(&1.0)).collect();
    let total: f64 = ws.iter().sum();
    let mut x = rng.gen::<f64>() * total;
    for (t, wt) in tags.iter().zip(ws) {
        if x < wt {
            return t;
        }
        x -= wt;
    }
    tags[0]
}

// ---------- Jadwal / status ----------

static FORCE_NOW: AtomicBool = AtomicBool::new(false);
static SNOOZE_UNTIL: AtomicI64 = AtomicI64::new(0);
static SNOOZES_TODAY: AtomicI64 = AtomicI64::new(0);
static SNOOZE_DAY: AtomicI64 = AtomicI64::new(0);

fn flag() -> std::path::PathBuf {
    crate::config::data_dir().join("bec.on")
}

pub fn enabled() -> bool {
    flag().exists()
}

pub fn set_enabled(on: bool) {
    if on {
        let _ = std::fs::write(flag(), "1");
    } else {
        let _ = std::fs::remove_file(flag());
    }
}

pub fn force_now() {
    FORCE_NOW.store(true, Ordering::Relaxed);
}

/// Hari tes: sebelum jam 04:00 masih dihitung malam sebelumnya.
fn test_day() -> String {
    (Local::now() - Cd::hours(4)).format("%Y-%m-%d").to_string()
}

fn done_today(c: &Connection) -> bool {
    c.query_row("SELECT COUNT(*) FROM bec_log2 WHERE day=?1", [test_day()], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
}

pub fn done_today_pub() -> bool {
    db().map(|c| done_today(&c)).unwrap_or(true)
}

/// Jam tes (menit sejak 00:00): bec_time di jadwal.json, bawaan 22:00.
pub fn test_minute() -> i32 {
    let (h, m) = crate::assistant::schedule().bec_time.split_once(':').map(|(a, b)| (a.parse::<i32>().unwrap_or(22), b.parse::<i32>().unwrap_or(0))).unwrap_or((22, 0));
    h * 60 + m
}

/// Murni (mudah diuji): apakah tes harus tampil sekarang?
pub fn is_due(enabled: bool, now_min: i32, test_min: i32, done: bool, snoozed: bool, forced: bool) -> bool {
    if forced && !done {
        return true;
    }
    if !enabled || done || snoozed {
        return false;
    }
    now_min >= test_min || now_min < 4 * 60
}

pub fn due() -> bool {
    let Ok(c) = db() else { return false };
    let now = Local::now();
    let snoozed = SNOOZE_UNTIL.load(Ordering::Relaxed) > now.timestamp();
    is_due(enabled(), (now.hour() * 60 + now.minute()) as i32, test_minute(), done_today(&c), snoozed, FORCE_NOW.load(Ordering::Relaxed))
}

pub fn snooze() -> bool {
    let day = (Local::now() - Cd::hours(4)).date_naive().num_days_from_ce() as i64;
    if SNOOZE_DAY.swap(day, Ordering::Relaxed) != day {
        SNOOZES_TODAY.store(0, Ordering::Relaxed);
    }
    if SNOOZES_TODAY.load(Ordering::Relaxed) >= 2 {
        return false;
    }
    SNOOZES_TODAY.fetch_add(1, Ordering::Relaxed);
    FORCE_NOW.store(false, Ordering::Relaxed);
    SNOOZE_UNTIL.store(Local::now().timestamp() + 600, Ordering::Relaxed);
    true
}

fn level(c: &Connection) -> u8 {
    let rows: Vec<f64> = c
        .prepare("SELECT (l_ok+r_ok+g_ok)*1.0/NULLIF(l_n+r_n+g_n,0) FROM bec_log2 WHERE skipped=0 ORDER BY ts DESC LIMIT 3")
        .and_then(|mut st| st.query_map([], |r| r.get::<_, Option<f64>>(0)).map(|r| r.flatten().flatten().collect()))
        .unwrap_or_default();
    let cur: u8 = c.query_row("SELECT level FROM bec_log2 ORDER BY ts DESC LIMIT 1", [], |r| r.get::<_, i64>(0)).map(|x| x as u8).unwrap_or(1);
    if rows.len() >= 3 {
        let avg = rows.iter().sum::<f64>() / rows.len() as f64;
        if avg >= 0.8 && cur < 2 {
            return 2;
        }
        if avg < 0.5 && cur > 1 {
            return 1;
        }
    }
    cur.max(1)
}

fn streak(c: &Connection) -> u32 {
    let days: Vec<String> = c
        .prepare("SELECT day FROM bec_log2 WHERE skipped=0 ORDER BY day DESC LIMIT 60")
        .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    let mut d = (Local::now() - Cd::hours(4)).date_naive();
    let mut n = 0;
    if !days.contains(&d.format("%Y-%m-%d").to_string()) {
        d -= Cd::days(1);
    }
    while days.contains(&d.format("%Y-%m-%d").to_string()) {
        n += 1;
        d -= Cd::days(1);
    }
    n
}

/// Apakah Windows punya suara Inggris? (dicek sekali)
pub fn tts_available() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *C.get_or_init(|| {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-Command", "Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; ($s.GetInstalledVoices() | ? { $_.VoiceInfo.Culture.Name -like 'en-*' -and $_.Enabled }).Count"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<i32>().unwrap_or(0) > 0).unwrap_or(false)
    })
}

/// Putar teks dengan suara Inggris offline Windows (SAPI). Mengembalikan proses agar bisa dihentikan.
pub fn speak(script: &str) -> Option<std::process::Child> {
    let safe = script.replace('\'', "''").replace('\n', " ");
    let ps = format!("Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $v=$s.GetInstalledVoices() | ? {{ $_.VoiceInfo.Culture.Name -like 'en-*' -and $_.Enabled }} | select -First 1; if($v){{$s.SelectVoice($v.VoiceInfo.Name)}}; $s.Rate=-1; $s.Speak('{safe}')");
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-Command", &ps]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.spawn().ok()
}

fn per_section() -> usize {
    crate::assistant::schedule().bec_questions.clamp(5, 60)
}

fn dedupe_fill(section: &str, n: usize, lvl: u8, hand: &[Item], w: &HashMap<String, f64>, rng: &mut impl Rng) -> Vec<Item> {
    let max_hand = if section == "listening" { 0 } else { n / 3 };
    let mut out: Vec<Item> = hand.iter().filter(|i| i.section == section).take(max_hand).cloned().collect();
    let tags: &[&'static str] = match section {
        "listening" => &["numbers", "time", "place", "intent", "detail"],
        "reading" => &["numbers", "purpose", "inference", "detail"],
        _ => &["preposition", "collocation", "linker", "comparison", "quantifier", "tense"],
    };
    let mut guard = 0;
    while out.len() < n && guard < n * 60 {
        guard += 1;
        let tag = weighted_tag(rng, section, tags, w);
        let it = match section {
            "listening" => gen_listening(tag, rng),
            "reading" => gen_reading(tag, lvl, rng),
            _ => gen_grammar(tag, lvl, rng),
        };
        if !out.iter().any(|x| x.q == it.q && x.passage == it.passage && x.audio == it.audio) {
            out.push(it);
        }
    }
    out.shuffle(rng);
    out
}

/// Susun tes harian: n soal per bagian (bawaan 30). Soal tulisan tangan yang pernah salah didahulukan; sisanya dibangkitkan
/// dengan bobot ke tag yang lemah.
pub fn build_test() -> TestSet {
    let mut rng = rand::thread_rng();
    let n = per_section();
    let (lv, weights, skip_ok, strk, wrong_ids) = match db() {
        Ok(c) => {
            let skips: i64 = c.query_row("SELECT COUNT(*) FROM bec_log2 WHERE skipped=1 AND day >= date('now','-7 day')", [], |r| r.get(0)).unwrap_or(0);
            let wrong: std::collections::HashSet<String> = c
                .prepare("SELECT id FROM bec_stat WHERE wrong>0")
                .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
                .unwrap_or_default();
            (level(&c), tag_weights(&c), skips < 2, streak(&c), wrong)
        }
        Err(_) => (1, HashMap::new(), true, 0, Default::default()),
    };
    let mut hand: Vec<Item> = BANK.iter().filter(|q| q.lvl <= lv).map(|q| hand_item(q, &mut rng)).collect();
    hand.shuffle(&mut rng);
    hand.sort_by_key(|i| !wrong_ids.contains(&i.id)); // yang pernah salah dulu
    TestSet {
        listening: dedupe_fill("listening", n, lv, &hand, &weights, &mut rng),
        reading: dedupe_fill("reading", n, lv, &hand, &weights, &mut rng),
        grammar: dedupe_fill("grammar", n, lv, &hand, &weights, &mut rng),
        level: lv,
        streak: strk,
        skip_ok,
        snooze_ok: SNOOZES_TODAY.load(Ordering::Relaxed) < 2,
        tts_ok: tts_available(),
    }
}

// ---------- Evaluasi ----------

fn lesson(tag: &str) -> (&'static str, &'static str) {
    match tag {
        "numbers" => ("Angka & detail (numbers)", "Saat mendengar/membaca angka, tulis dulu di kertas. Latihan: dengarkan nomor telepon, harga, jam; ulangi keras-keras digit per digit. Waspadai pasangan 13/30, 15/50."),
        "time" => ("Jam & jadwal (time)", "Kuasai pola 'moved from A to B', 'delayed', 'now leaves at'. Jawaban biasanya waktu yang BARU, bukan yang lama."),
        "place" => ("Tempat & petunjuk (place)", "Dengarkan frasa pengganti: 'in the meantime', 'instead', 'use ... floor'. Pilih lokasi alternatif, bukan lokasi masalah."),
        "intent" => ("Maksud pembicara (intent)", "Perhatikan penolakan lalu usulan baru ('I'm sorry... How about...'). Kesepakatan ada di bagian akhir percakapan."),
        "detail" => ("Detail spesifik (detail)", "Garis bawahi kata kunci pertanyaan (apa/kapan/siapa) lalu cari sinonimnya di teks. Jangan memilih jawaban hanya karena kata yang sama persis."),
        "purpose" => ("Tujuan teks (purpose)", "Baca kalimat pembuka & penutup email: di sanalah tujuan (To inform / To request / To complain / To invite) biasanya ada."),
        "inference" => ("Menyimpulkan (inference)", "Cari makna tersirat: 'the rest or return my money' = ingin sisa barang atau pengembalian dana. Tanyakan: apa yang penulis INGINKAN?"),
        "functional" => ("Bahasa fungsional", "Hafal frasa sopan: Could you possibly..., I'm afraid..., Would you mind..., Shall we...; dan balasan standarnya (Not at all, That suits me)."),
        "preposition" => ("Preposisi", "Belajar sebagai paket (kolokasi): interested IN, responsible FOR, depend ON, apply FOR, agree WITH, proud OF. Buat kartu 5 per hari."),
        "collocation" => ("Kolokasi make/do/take/give", "make a decision/appointment/profit, take a break/part, give a presentation/feedback, hold a meeting, meet a deadline, reach an agreement."),
        "linker" => ("Kata penghubung", "because + klausa, because of + kata benda; although + klausa, despite + kata benda/-ing; so = akibat; while/during/until/as soon as untuk waktu."),
        "comparison" => ("Perbandingan", "Adjektiva pendek + er (cheaper), panjang: more + adj (more reliable). Tak beraturan: good-better-best, bad-worse-worst."),
        "quantifier" => ("Quantifier (much/many/any/few)", "much + tak terhitung (money, time, information), many + terhitung (emails, files). any dipakai di kalimat negatif/pertanyaan."),
        "tense" => ("Tenses", "Past simple untuk waktu selesai (yesterday), present perfect untuk 'yet/already/since/for', past perfect untuk kejadian sebelum kejadian lampau lain, will untuk rencana/prediksi."),
        "conditional" => ("Conditional", "If + present simple, will + verb (nyata); If + past simple, would + verb (tidak nyata, 'If I were you...')."),
        "passive" => ("Passive voice", "be + past participle: 'The goods were sent', 'The report was approved'. Pakai saat pelaku tidak penting."),
        "verb_pattern" => ("Pola kata kerja", "suggest/enjoy/finish + -ing; look forward TO + -ing; let/make + verb dasar; want/need + to-infinitive."),
        "reported_speech" => ("Reported speech", "Pertanyaan ya/tidak dilaporkan dengan if/whether, urutan kata seperti kalimat biasa: 'She asked if I had finished.'"),
        "relative" => ("Relative clause", "who untuk orang, which untuk benda, whose untuk kepemilikan."),
        "vocabulary" => ("Kosakata bisnis", "Pelajari 5 kata baru per hari dalam konteks kalimat (postpone, quotation, comply, payroll, discount). Tulis satu kalimat milikmu sendiri untuk tiap kata."),
        _ => ("Umum", "Ulangi soal yang salah dan baca penjelasannya."),
    }
}

fn band(pct: f32) -> &'static str {
    match pct {
        p if p >= 90.0 => "C1 (Higher)",
        p if p >= 75.0 => "B2 (Vantage)",
        p if p >= 55.0 => "B1+ (menuju Vantage)",
        p if p >= 35.0 => "B1 (Preliminary)",
        _ => "A2 (perlu dasar kuat)",
    }
}

/// Evaluasi untuk satu sesi (murni): skor per bagian, level, kelemahan, dan saran.
pub fn evaluate(ans: &[Ans]) -> String {
    let mut s = String::from("📊 EVALUASI TES BEC\n");
    let mut total_ok = 0;
    for (sec, label) in [("listening", "Listening"), ("reading", "Reading"), ("grammar", "Grammar")] {
        let v: Vec<&Ans> = ans.iter().filter(|a| a.section == sec).collect();
        if v.is_empty() {
            continue;
        }
        let ok = v.iter().filter(|a| a.ok).count();
        total_ok += ok;
        let pct = ok as f32 * 100.0 / v.len() as f32;
        s.push_str(&format!("• {label}: {ok}/{} ({pct:.0}%) ≈ {}\n", v.len(), band(pct)));
    }
    let tot = ans.len().max(1);
    let pct = total_ok as f32 * 100.0 / tot as f32;
    s.push_str(&format!("Total: {total_ok}/{} ({pct:.0}%) ≈ {}\n", ans.len(), band(pct)));
    let mut by: HashMap<(String, String), (usize, usize)> = HashMap::new();
    for a in ans {
        let e = by.entry((a.section.clone(), a.tag.clone())).or_default();
        e.0 += 1;
        if !a.ok {
            e.1 += 1;
        }
    }
    let mut weak: Vec<(&(String, String), &(usize, usize))> = by.iter().filter(|(_, v)| v.1 >= 2 && v.1 as f32 / v.0 as f32 >= 0.4).collect();
    weak.sort_by(|a, b| (b.1 .1 as f32 / b.1 .0 as f32).total_cmp(&(a.1 .1 as f32 / a.1 .0 as f32)).then(b.1 .1.cmp(&a.1 .1)));
    let mut strong: Vec<String> = by.iter().filter(|(_, v)| v.0 >= 3 && v.1 == 0).map(|(k, _)| lesson(&k.1).0.to_string()).collect();
    strong.sort();
    strong.dedup();
    strong.truncate(3);
    if weak.is_empty() {
        s.push_str("✅ Tidak ada kelemahan yang menonjol malam ini. Pertahankan!\n");
    } else {
        s.push_str("⚠ Kesulitanmu:\n");
        for ((sec, tag), (n, wrong)) in weak.iter().take(4) {
            let (name, tip) = lesson(tag);
            s.push_str(&format!("• {name} [{sec}] salah {wrong}/{n}\n   → {tip}\n"));
        }
    }
    if !strong.is_empty() {
        s.push_str(&format!("💪 Sudah kuat: {}\n", strong.join(", ")));
    }
    s.push_str("📚 Saran: 15 menit/hari untuk 2 kelemahan teratas di atas; besok soal pada tag itu kuperbanyak otomatis.");
    s
}

#[derive(Deserialize, Clone)]
pub struct Hasil {
    pub answers: Vec<Ans>,
}

/// Simpan hasil: statistik per tag + per soal + log harian. Mengembalikan ringkasan satu baris.
pub fn record(r: &Hasil, skipped: bool) -> String {
    let Ok(c) = db() else { return "gagal menyimpan hasil".into() };
    let cnt = |sec: &str| (r.answers.iter().filter(|a| a.section == sec && a.ok).count() as i64, r.answers.iter().filter(|a| a.section == sec).count() as i64);
    for a in &r.answers {
        let _ = c.execute("INSERT INTO bec_tag VALUES(?1,?2,1,?3) ON CONFLICT(section,tag) DO UPDATE SET seen=seen+1, wrong=wrong+?3", (&a.section, &a.tag, !a.ok as i64));
        if !a.id.starts_with("gen:") {
            let _ = c.execute(
                "INSERT INTO bec_stat VALUES(?1,1,?2,?3) ON CONFLICT(id) DO UPDATE SET seen=seen+1, wrong=wrong+?2, last_wrong=?3",
                (&a.id, !a.ok as i64, if a.ok { 0 } else { Local::now().timestamp() }),
            );
        }
    }
    let (l, r2, g) = (cnt("listening"), cnt("reading"), cnt("grammar"));
    let lv = level(&c);
    let _ = c.execute("INSERT OR REPLACE INTO bec_log2 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", (test_day(), l.0, l.1, r2.0, r2.1, g.0, g.1, lv as i64, skipped as i64, Local::now().timestamp()));
    FORCE_NOW.store(false, Ordering::Relaxed);
    if skipped {
        return "Tes BEC dilewati (darurat) dan dicatat.".into();
    }
    format!("Tes BEC selesai: Listening {}/{}, Reading {}/{}, Grammar {}/{} · level {lv} · streak {} hari.", l.0, l.1, r2.0, r2.1, g.0, g.1, streak(&c))
}

/// Peta kelemahan jangka panjang (dari semua tes) + tren harian, untuk /bec.
pub fn report() -> String {
    let Ok(c) = db() else { return "data BEC tidak tersedia".into() };
    let mut s = format!(
        "📘 Tes BEC harian: {} · jam {:02}:{:02} · {} soal/bagian · level {} · streak {} hari.\n",
        if enabled() { "AKTIF (layar terkunci sampai selesai)" } else { "mati (ketik /bec on)" },
        test_minute() / 60,
        test_minute() % 60,
        per_section(),
        level(&c),
        streak(&c)
    );
    let rows: Vec<(String, i64, i64, i64, i64, i64, i64, i64)> = c
        .prepare("SELECT day, l_ok, l_n, r_ok, r_n, g_ok, g_n, skipped FROM bec_log2 ORDER BY day DESC LIMIT 7")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    for (d, lo, ln, ro, rn, go, gn, sk) in rows {
        s.push_str(&if sk == 1 { format!("• {d}: dilewati (darurat)\n") } else { format!("• {d}: L {lo}/{ln} · R {ro}/{rn} · G {go}/{gn}\n") });
    }
    let weak: Vec<(String, String, i64, i64)> = c
        .prepare("SELECT section, tag, seen, wrong FROM bec_tag WHERE seen>=4 AND wrong>0 ORDER BY wrong*1.0/seen DESC, wrong DESC LIMIT 3")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    if !weak.is_empty() {
        s.push_str("Kelemahan jangka panjang:\n");
        for (sec, tag, seen, wrong) in weak {
            s.push_str(&format!("• {} [{sec}] salah {wrong}/{seen} → {}\n", lesson(&tag).0, lesson(&tag).1));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hand_bank_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for q in BANK {
            assert!(ids.insert(q.id), "id ganda {}", q.id);
        }
        assert!(BANK.len() >= 60);
        let mut rng = rand::thread_rng();
        for q in BANK {
            let it = hand_item(q, &mut rng);
            assert_eq!(it.opts[it.ans], q.opts[0]);
            assert_eq!(it.opts.len(), 4);
        }
    }
    #[test]
    fn generators_make_valid_items() {
        let mut rng = rand::thread_rng();
        for _ in 0..200 {
            for (sec, tags) in [("listening", &["numbers", "time", "place", "intent", "detail"][..]), ("reading", &["numbers", "purpose", "inference", "detail"]), ("grammar", &["preposition", "collocation", "linker", "comparison", "quantifier", "tense"])] {
                for t in tags {
                    let it = match sec {
                        "listening" => gen_listening(t, &mut rng),
                        "reading" => gen_reading(t, 2, &mut rng),
                        _ => gen_grammar(t, 2, &mut rng),
                    };
                    assert_eq!(it.opts.len(), 4, "{sec}/{t}: {:?}", it.opts);
                    assert!(it.ans < 4);
                    let mut o = it.opts.clone();
                    o.sort();
                    o.dedup();
                    assert_eq!(o.len(), 4, "pilihan ganda: {} {:?}", it.q, it.opts);
                    assert!(!it.q.is_empty());
                    if sec == "listening" {
                        assert!(!it.audio.is_empty());
                    }
                }
            }
        }
    }
    #[test]
    fn test_set_has_30_30_30_without_duplicates() {
        let t = build_test();
        for v in [&t.listening, &t.reading, &t.grammar] {
            assert_eq!(v.len(), 30);
            let mut qs: Vec<String> = v.iter().map(|i| format!("{}|{}|{}", i.q, i.passage, i.audio)).collect();
            qs.sort();
            qs.dedup();
            assert_eq!(qs.len(), 30);
        }
        assert!(t.listening.iter().all(|i| i.section == "listening"));
    }
    #[test]
    fn due_logic() {
        let t = 22 * 60;
        assert!(!is_due(false, 23 * 60, t, false, false, false));
        assert!(!is_due(true, 21 * 60, t, false, false, false));
        assert!(is_due(true, 22 * 60 + 5, t, false, false, false));
        assert!(is_due(true, 60, t, false, false, false));
        assert!(!is_due(true, 23 * 60, t, true, false, false));
        assert!(!is_due(true, 23 * 60, t, false, true, false));
        assert!(is_due(false, 10 * 60, t, false, false, true));
    }
    #[test]
    fn evaluation_finds_weak_tags() {
        let mut ans = vec![];
        for i in 0..6 {
            ans.push(Ans { id: format!("a{i}"), section: "grammar".into(), tag: "preposition".into(), ok: i % 3 == 0 });
        }
        for i in 0..5 {
            ans.push(Ans { id: format!("b{i}"), section: "listening".into(), tag: "numbers".into(), ok: true });
        }
        let e = evaluate(&ans);
        assert!(e.contains("Preposisi") && e.contains("Listening: 5/5") && e.contains("Grammar: 2/6"), "{e}");
        assert!(e.contains("Sudah kuat"));
    }
}
