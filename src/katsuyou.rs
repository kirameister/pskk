use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub type Conjugation = (String, String, i32);

static COST_PENALTIES: OnceLock<HashMap<&'static str, i32>> = OnceLock::new();
static GODAN_STEM_SUFFIXES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static GODAN_SUFFIXES: OnceLock<HashMap<&'static str, HashMap<&'static str, &'static str>>> =
    OnceLock::new();
static ICHIDAN_SUFFIXES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static KEIYOUSHI_SUFFIXES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static SPECIAL_TE_TA: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> =
    OnceLock::new();
static SKK_OKURIGANA_MAP: OnceLock<HashMap<char, (&'static str, &'static str)>> = OnceLock::new();

const ICHIDAN_STEM_SUFFIX: &str = "る";
const KEIYOUSHI_STEM_SUFFIX: &str = "い";

fn cost_penalties() -> &'static HashMap<&'static str, i32> {
    COST_PENALTIES.get_or_init(|| {
        HashMap::from([
            ("終止形", 0),
            ("連体形", 0),
            ("連用形", 50),
            ("未然形", 50),
            ("仮定形", 50),
            ("命令形", 50),
            ("て形", 100),
            ("た形", 100),
            ("ない形", 150),
            ("ます形", 150),
            ("ば形", 150),
            ("たい形", 150),
            ("られる形", 200),
            ("れる形", 200),
            ("させる形", 200),
            ("せる形", 200),
        ])
    })
}

fn godan_stem_suffixes() -> &'static HashMap<&'static str, &'static str> {
    GODAN_STEM_SUFFIXES.get_or_init(|| {
        HashMap::from([
            ("五段-カ行", "く"),
            ("五段-ガ行", "ぐ"),
            ("五段-サ行", "す"),
            ("五段-タ行", "つ"),
            ("五段-ナ行", "ぬ"),
            ("五段-バ行", "ぶ"),
            ("五段-マ行", "む"),
            ("五段-ラ行", "る"),
            ("五段-ワア行", "う"),
        ])
    })
}

fn godan_suffixes() -> &'static HashMap<&'static str, HashMap<&'static str, &'static str>> {
    GODAN_SUFFIXES.get_or_init(|| {
        HashMap::from([
            (
                "カ行",
                HashMap::from([
                    ("未然形", "か"),
                    ("連用形", "き"),
                    ("終止形", "く"),
                    ("連体形", "く"),
                    ("仮定形", "け"),
                    ("命令形", "け"),
                    ("て形", "いて"),
                    ("た形", "いた"),
                ]),
            ),
            (
                "ガ行",
                HashMap::from([
                    ("未然形", "が"),
                    ("連用形", "ぎ"),
                    ("終止形", "ぐ"),
                    ("連体形", "ぐ"),
                    ("仮定形", "げ"),
                    ("命令形", "げ"),
                    ("て形", "いで"),
                    ("た形", "いだ"),
                ]),
            ),
            (
                "サ行",
                HashMap::from([
                    ("未然形", "さ"),
                    ("連用形", "し"),
                    ("終止形", "す"),
                    ("連体形", "す"),
                    ("仮定形", "せ"),
                    ("命令形", "せ"),
                    ("て形", "して"),
                    ("た形", "した"),
                ]),
            ),
            (
                "タ行",
                HashMap::from([
                    ("未然形", "た"),
                    ("連用形", "ち"),
                    ("終止形", "つ"),
                    ("連体形", "つ"),
                    ("仮定形", "て"),
                    ("命令形", "て"),
                    ("て形", "って"),
                    ("た形", "った"),
                ]),
            ),
            (
                "ナ行",
                HashMap::from([
                    ("未然形", "な"),
                    ("連用形", "に"),
                    ("終止形", "ぬ"),
                    ("連体形", "ぬ"),
                    ("仮定形", "ね"),
                    ("命令形", "ね"),
                    ("て形", "んで"),
                    ("た形", "んだ"),
                ]),
            ),
            (
                "バ行",
                HashMap::from([
                    ("未然形", "ば"),
                    ("連用形", "び"),
                    ("終止形", "ぶ"),
                    ("連体形", "ぶ"),
                    ("仮定形", "べ"),
                    ("命令形", "べ"),
                    ("て形", "んで"),
                    ("た形", "んだ"),
                ]),
            ),
            (
                "マ行",
                HashMap::from([
                    ("未然形", "ま"),
                    ("連用形", "み"),
                    ("終止形", "む"),
                    ("連体形", "む"),
                    ("仮定形", "め"),
                    ("命令形", "め"),
                    ("て形", "んで"),
                    ("た形", "んだ"),
                ]),
            ),
            (
                "ラ行",
                HashMap::from([
                    ("未然形", "ら"),
                    ("連用形", "り"),
                    ("終止形", "る"),
                    ("連体形", "る"),
                    ("仮定形", "れ"),
                    ("命令形", "れ"),
                    ("て形", "って"),
                    ("た形", "った"),
                ]),
            ),
            (
                "ワア行",
                HashMap::from([
                    ("未然形", "わ"),
                    ("連用形", "い"),
                    ("終止形", "う"),
                    ("連体形", "う"),
                    ("仮定形", "え"),
                    ("命令形", "え"),
                    ("て形", "って"),
                    ("た形", "った"),
                ]),
            ),
        ])
    })
}

fn ichidan_suffixes() -> &'static HashMap<&'static str, &'static str> {
    ICHIDAN_SUFFIXES.get_or_init(|| {
        HashMap::from([
            ("未然形", ""),
            ("連用形", ""),
            ("終止形", "る"),
            ("連体形", "る"),
            ("仮定形", "れ"),
            ("命令形_ろ", "ろ"),
            ("命令形_よ", "よ"),
            ("て形", "て"),
            ("た形", "た"),
        ])
    })
}

fn keiyoushi_suffixes() -> &'static HashMap<&'static str, &'static str> {
    KEIYOUSHI_SUFFIXES.get_or_init(|| {
        HashMap::from([
            ("連用形", "く"),
            ("終止形", "い"),
            ("連体形", "い"),
            ("仮定形", "けれ"),
            ("て形", "くて"),
            ("た形", "かった"),
        ])
    })
}

fn special_te_ta() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    SPECIAL_TE_TA.get_or_init(|| HashMap::from([("いく", ("いって", "いった")), ("ゆく", ("ゆって", "ゆった"))]))
}

fn skk_okurigana_map() -> &'static HashMap<char, (&'static str, &'static str)> {
    SKK_OKURIGANA_MAP.get_or_init(|| {
        HashMap::from([
            ('k', ("く", "五段-カ行")),
            ('g', ("ぐ", "五段-ガ行")),
            ('s', ("す", "五段-サ行")),
            ('t', ("つ", "五段-タ行")),
            ('n', ("ぬ", "五段-ナ行")),
            ('b', ("ぶ", "五段-バ行")),
            ('m', ("む", "五段-マ行")),
            ('r', ("る", "五段-ラ行")),
            ('w', ("う", "五段-ワア行")),
            ('u', ("う", "五段-ワア行")),
            ('i', ("い", "形容詞")),
        ])
    })
}

pub fn is_conjugatable(conj_type: &str) -> bool {
    if conj_type.is_empty() || conj_type == "*" {
        return false;
    }
    godan_stem_suffixes().contains_key(conj_type)
        || conj_type.starts_with("上一段-")
        || conj_type.starts_with("下一段-")
        || matches!(conj_type, "サ行変格" | "カ行変格" | "形容詞")
}

pub fn get_row(conj_type: &str) -> String {
    conj_type
        .split_once('-')
        .map(|(_, row)| row.to_string())
        .unwrap_or_else(|| conj_type.to_string())
}

pub fn extract_stem(reading: &str, lemma: &str, conj_type: &str) -> (String, String) {
    if let Some(suffix) = godan_stem_suffixes().get(conj_type) {
        if reading.ends_with(suffix) {
            return (
                trim_chars_from_end(reading, suffix.chars().count()),
                trim_chars_from_end(lemma, 1),
            );
        }
    }

    if conj_type.starts_with("上一段-") || conj_type.starts_with("下一段-") {
        if reading.ends_with(ICHIDAN_STEM_SUFFIX) {
            return (trim_chars_from_end(reading, 1), trim_chars_from_end(lemma, 1));
        }
    }

    if conj_type == "サ行変格" {
        if reading.ends_with("する") || reading.ends_with("ずる") {
            return (trim_chars_from_end(reading, 2), trim_chars_from_end(lemma, 2));
        }
    }

    if conj_type == "カ行変格" && reading.ends_with("くる") {
        return (trim_chars_from_end(reading, 2), trim_chars_from_end(lemma, 1));
    }

    if conj_type == "形容詞" && reading.ends_with(KEIYOUSHI_STEM_SUFFIX) {
        return (trim_chars_from_end(reading, 1), trim_chars_from_end(lemma, 1));
    }

    (reading.to_string(), lemma.to_string())
}

pub fn generate_conjugations(
    reading: &str,
    lemma: &str,
    _pos: &str,
    conj_type: &str,
    base_cost: i32,
) -> Vec<Conjugation> {
    if !is_conjugatable(conj_type) {
        return vec![(reading.to_string(), lemma.to_string(), base_cost)];
    }

    if godan_stem_suffixes().contains_key(conj_type) {
        return conjugate_godan(reading, lemma, conj_type, base_cost);
    }
    if conj_type.starts_with("上一段-") || conj_type.starts_with("下一段-") {
        return conjugate_ichidan(reading, lemma, conj_type, base_cost);
    }
    if conj_type == "サ行変格" {
        return conjugate_suru(reading, lemma, base_cost);
    }
    if conj_type == "カ行変格" {
        return conjugate_kuru(reading, lemma, base_cost);
    }
    if conj_type == "形容詞" {
        return conjugate_keiyoushi(reading, lemma, base_cost);
    }

    vec![(reading.to_string(), lemma.to_string(), base_cost)]
}

fn conjugate_godan(reading: &str, lemma: &str, conj_type: &str, base_cost: i32) -> Vec<Conjugation> {
    let row = get_row(conj_type);
    let Some(suffixes) = godan_suffixes().get(row.as_str()) else {
        return vec![(reading.to_string(), lemma.to_string(), base_cost)];
    };

    let (stem_r, stem_l) = extract_stem(reading, lemma, conj_type);
    let mut results = Vec::new();
    let is_special = special_te_ta().contains_key(reading);

    for (form_name, suffix) in suffixes {
        let (conj_r, conj_l) = if is_special && matches!(*form_name, "て形" | "た形") {
            let (special_te, special_ta) = special_te_ta()[reading];
            if *form_name == "て形" {
                (special_te.to_string(), format!("{stem_l}って"))
            } else {
                (special_ta.to_string(), format!("{stem_l}った"))
            }
        } else {
            (format!("{stem_r}{suffix}"), format!("{stem_l}{suffix}"))
        };
        results.push((conj_r, conj_l, base_cost + penalty(form_name)));
    }

    let mizen_suffix = suffixes.get("未然形").copied().unwrap_or_default();
    let renyo_suffix = suffixes.get("連用形").copied().unwrap_or_default();
    let katei_suffix = suffixes.get("仮定形").copied().unwrap_or_default();

    results.push(compound(&stem_r, &stem_l, mizen_suffix, "ない", base_cost + penalty("ない形")));
    results.push(compound(&stem_r, &stem_l, renyo_suffix, "ます", base_cost + penalty("ます形")));
    results.push(compound(&stem_r, &stem_l, katei_suffix, "ば", base_cost + penalty("ば形")));
    results.push(compound(&stem_r, &stem_l, renyo_suffix, "たい", base_cost + penalty("たい形")));
    results.push(compound(&stem_r, &stem_l, mizen_suffix, "れる", base_cost + penalty("れる形")));
    results.push(compound(&stem_r, &stem_l, mizen_suffix, "せる", base_cost + penalty("せる形")));

    results
}

fn conjugate_ichidan(reading: &str, lemma: &str, conj_type: &str, base_cost: i32) -> Vec<Conjugation> {
    let (stem_r, stem_l) = extract_stem(reading, lemma, conj_type);
    let mut results = Vec::new();

    for (form_name, suffix) in ichidan_suffixes() {
        let cost_key = form_name.split('_').next().unwrap_or(form_name);
        results.push((
            format!("{stem_r}{suffix}"),
            format!("{stem_l}{suffix}"),
            base_cost + penalty(cost_key),
        ));
    }

    results.push(compound(&stem_r, &stem_l, "", "ない", base_cost + penalty("ない形")));
    results.push(compound(&stem_r, &stem_l, "", "ます", base_cost + penalty("ます形")));
    results.push(compound(&stem_r, &stem_l, "", "れば", base_cost + penalty("ば形")));
    results.push(compound(&stem_r, &stem_l, "", "たい", base_cost + penalty("たい形")));
    results.push(compound(&stem_r, &stem_l, "", "られる", base_cost + penalty("られる形")));
    results.push(compound(&stem_r, &stem_l, "", "させる", base_cost + penalty("させる形")));

    results
}

fn conjugate_suru(reading: &str, lemma: &str, base_cost: i32) -> Vec<Conjugation> {
    let (stem_r, stem_l, is_zuru) = if reading.ends_with("する") {
        (
            trim_chars_from_end(reading, 2),
            trim_chars_from_end(lemma, 2),
            false,
        )
    } else if reading.ends_with("ずる") {
        (
            trim_chars_from_end(reading, 2),
            trim_chars_from_end(lemma, 2),
            true,
        )
    } else {
        return vec![(reading.to_string(), lemma.to_string(), base_cost)];
    };

    let base = if is_zuru { "ずる" } else { "する" };
    vec![
        (format!("{stem_r}{base}"), format!("{stem_l}{base}"), base_cost),
        (format!("{stem_r}し"), format!("{stem_l}し"), base_cost + penalty("連用形")),
        (format!("{stem_r}さ"), format!("{stem_l}さ"), base_cost + penalty("未然形")),
        (format!("{stem_r}せ"), format!("{stem_l}せ"), base_cost + penalty("未然形")),
        (format!("{stem_r}すれ"), format!("{stem_l}すれ"), base_cost + penalty("仮定形")),
        (format!("{stem_r}しろ"), format!("{stem_l}しろ"), base_cost + penalty("命令形")),
        (format!("{stem_r}せよ"), format!("{stem_l}せよ"), base_cost + penalty("命令形")),
        (format!("{stem_r}して"), format!("{stem_l}して"), base_cost + penalty("て形")),
        (format!("{stem_r}した"), format!("{stem_l}した"), base_cost + penalty("た形")),
        (format!("{stem_r}しない"), format!("{stem_l}しない"), base_cost + penalty("ない形")),
        (format!("{stem_r}します"), format!("{stem_l}します"), base_cost + penalty("ます形")),
        (format!("{stem_r}すれば"), format!("{stem_l}すれば"), base_cost + penalty("ば形")),
        (format!("{stem_r}したい"), format!("{stem_l}したい"), base_cost + penalty("たい形")),
        (format!("{stem_r}される"), format!("{stem_l}される"), base_cost + penalty("られる形")),
        (format!("{stem_r}させる"), format!("{stem_l}させる"), base_cost + penalty("させる形")),
    ]
}

fn conjugate_kuru(reading: &str, lemma: &str, base_cost: i32) -> Vec<Conjugation> {
    if !reading.ends_with("くる") {
        return vec![(reading.to_string(), lemma.to_string(), base_cost)];
    }

    let prefix_r = trim_chars_from_end(reading, 2);
    let prefix_l = trim_chars_from_end(lemma, 1);

    vec![
        (format!("{prefix_r}くる"), format!("{prefix_l}る"), base_cost + penalty("終止形")),
        (format!("{prefix_r}き"), prefix_l.clone(), base_cost + penalty("連用形")),
        (format!("{prefix_r}こ"), prefix_l.clone(), base_cost + penalty("未然形")),
        (format!("{prefix_r}くれ"), format!("{prefix_l}れ"), base_cost + penalty("仮定形")),
        (format!("{prefix_r}こい"), format!("{prefix_l}い"), base_cost + penalty("命令形")),
        (format!("{prefix_r}きて"), format!("{prefix_l}て"), base_cost + penalty("て形")),
        (format!("{prefix_r}きた"), format!("{prefix_l}た"), base_cost + penalty("た形")),
        (format!("{prefix_r}こない"), format!("{prefix_l}ない"), base_cost + penalty("ない形")),
        (format!("{prefix_r}きます"), format!("{prefix_l}ます"), base_cost + penalty("ます形")),
        (format!("{prefix_r}くれば"), format!("{prefix_l}れば"), base_cost + penalty("ば形")),
        (format!("{prefix_r}きたい"), format!("{prefix_l}たい"), base_cost + penalty("たい形")),
        (format!("{prefix_r}こられる"), format!("{prefix_l}られる"), base_cost + penalty("られる形")),
        (format!("{prefix_r}こさせる"), format!("{prefix_l}させる"), base_cost + penalty("させる形")),
    ]
}

fn conjugate_keiyoushi(reading: &str, lemma: &str, base_cost: i32) -> Vec<Conjugation> {
    if !reading.ends_with('い') {
        return vec![(reading.to_string(), lemma.to_string(), base_cost)];
    }

    let stem_r = trim_chars_from_end(reading, 1);
    let stem_l = trim_chars_from_end(lemma, 1);
    let mut results = Vec::new();

    for (form_name, suffix) in keiyoushi_suffixes() {
        results.push((
            format!("{stem_r}{suffix}"),
            format!("{stem_l}{suffix}"),
            base_cost + penalty(form_name),
        ));
    }

    results.push((format!("{stem_r}くない"), format!("{stem_l}くない"), base_cost + penalty("ない形")));
    results.push((format!("{stem_r}ければ"), format!("{stem_l}ければ"), base_cost + penalty("ば形")));
    results
}

pub fn expand_skk_okurigana(reading: &str, kanji: &str, base_count: i32) -> Vec<Conjugation> {
    if reading.chars().count() < 2 {
        return Vec::new();
    }

    let suffix = reading.chars().last().unwrap();
    let Some((kana_suffix, conj_type)) = skk_okurigana_map().get(&suffix) else {
        return Vec::new();
    };

    let stem_r = trim_chars_from_end(reading, 1);
    let dict_reading = format!("{stem_r}{kana_suffix}");
    let dict_surface = format!("{kanji}{kana_suffix}");
    let pos = if *conj_type == "形容詞" { "形容詞" } else { "動詞" };

    let raw_results = generate_conjugations(&dict_reading, &dict_surface, pos, conj_type, 0);
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for (conj_reading, conj_surface, _) in raw_results {
        if seen.insert((conj_reading.clone(), conj_surface.clone())) {
            results.push((conj_reading, conj_surface, base_count));
        }
    }

    results
}

pub fn is_skk_okurigana_entry(reading: &str) -> bool {
    reading.chars().count() >= 2
        && reading
            .chars()
            .last()
            .is_some_and(|suffix| skk_okurigana_map().contains_key(&suffix))
}

fn penalty(form_name: &str) -> i32 {
    *cost_penalties().get(form_name).unwrap_or(&100)
}

fn compound(stem_r: &str, stem_l: &str, suffix: &str, ending: &str, cost: i32) -> Conjugation {
    (
        format!("{stem_r}{suffix}{ending}"),
        format!("{stem_l}{suffix}{ending}"),
        cost,
    )
}

fn trim_chars_from_end(input: &str, count: usize) -> String {
    let len = input.chars().count();
    input.chars().take(len.saturating_sub(count)).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        expand_skk_okurigana, extract_stem, generate_conjugations, get_row, is_conjugatable,
        is_skk_okurigana_entry,
    };
    use std::collections::HashSet;

    fn forms(results: &[(String, String, i32)]) -> HashSet<(String, String)> {
        results
            .iter()
            .map(|(r, s, _)| (r.clone(), s.clone()))
            .collect()
    }

    #[test]
    fn conjugatable_detection_matches_python_behavior() {
        assert!(is_conjugatable("五段-カ行"));
        assert!(is_conjugatable("下一段-バ行"));
        assert!(is_conjugatable("サ行変格"));
        assert!(is_conjugatable("カ行変格"));
        assert!(is_conjugatable("形容詞"));
        assert!(!is_conjugatable("*"));
        assert!(!is_conjugatable(""));
        assert!(!is_conjugatable("文語四段-カ行"));
    }

    #[test]
    fn row_extraction_matches() {
        assert_eq!(get_row("五段-カ行"), "カ行");
        assert_eq!(get_row("下一段-バ行"), "バ行");
        assert_eq!(get_row("形容詞"), "形容詞");
    }

    #[test]
    fn stem_extraction_matches() {
        assert_eq!(extract_stem("かく", "書く", "五段-カ行"), ("か".to_string(), "書".to_string()));
        assert_eq!(extract_stem("たべる", "食べる", "下一段-バ行"), ("たべ".to_string(), "食べ".to_string()));
        assert_eq!(extract_stem("あいする", "愛する", "サ行変格"), ("あい".to_string(), "愛".to_string()));
        assert_eq!(extract_stem("くる", "来る", "カ行変格"), ("".to_string(), "来".to_string()));
        assert_eq!(extract_stem("あかい", "赤い", "形容詞"), ("あか".to_string(), "赤".to_string()));
    }

    #[test]
    fn godan_conjugation_includes_expected_forms() {
        let results = generate_conjugations("かく", "書く", "動詞", "五段-カ行", 5000);
        let forms = forms(&results);

        for expected in [
            ("かく", "書く"),
            ("かき", "書き"),
            ("かか", "書か"),
            ("かけ", "書け"),
            ("かいて", "書いて"),
            ("かいた", "書いた"),
            ("かかない", "書かない"),
            ("かきます", "書きます"),
            ("かけば", "書けば"),
            ("かきたい", "書きたい"),
            ("かかれる", "書かれる"),
            ("かかせる", "書かせる"),
        ] {
            assert!(forms.contains(&(expected.0.to_string(), expected.1.to_string())));
        }
    }

    #[test]
    fn iku_special_case_uses_tte_tta() {
        let forms = forms(&generate_conjugations("いく", "行く", "動詞", "五段-カ行", 5000));
        assert!(forms.contains(&("いって".to_string(), "行って".to_string())));
        assert!(forms.contains(&("いった".to_string(), "行った".to_string())));
        assert!(!forms.contains(&("いいて".to_string(), "行いて".to_string())));
    }

    #[test]
    fn ichidan_conjugation_includes_expected_forms() {
        let forms = forms(&generate_conjugations("たべる", "食べる", "動詞", "下一段-バ行", 5000));
        for expected in [
            ("たべる", "食べる"),
            ("たべ", "食べ"),
            ("たべれ", "食べれ"),
            ("たべろ", "食べろ"),
            ("たべよ", "食べよ"),
            ("たべて", "食べて"),
            ("たべた", "食べた"),
            ("たべない", "食べない"),
            ("たべます", "食べます"),
            ("たべれば", "食べれば"),
            ("たべたい", "食べたい"),
            ("たべられる", "食べられる"),
            ("たべさせる", "食べさせる"),
        ] {
            assert!(forms.contains(&(expected.0.to_string(), expected.1.to_string())));
        }
    }

    #[test]
    fn suru_conjugation_includes_irregular_forms() {
        let forms = forms(&generate_conjugations("あいする", "愛する", "動詞", "サ行変格", 5000));
        for expected in [
            ("あいする", "愛する"),
            ("あいし", "愛し"),
            ("あいさ", "愛さ"),
            ("あいせ", "愛せ"),
            ("あいすれ", "愛すれ"),
            ("あいして", "愛して"),
            ("あいした", "愛した"),
            ("あいしない", "愛しない"),
            ("あいします", "愛します"),
            ("あいすれば", "愛すれば"),
            ("あいしたい", "愛したい"),
            ("あいされる", "愛される"),
            ("あいさせる", "愛させる"),
            ("あいしろ", "愛しろ"),
            ("あいせよ", "愛せよ"),
        ] {
            assert!(forms.contains(&(expected.0.to_string(), expected.1.to_string())));
        }
    }

    #[test]
    fn kuru_conjugation_changes_reading() {
        let forms = forms(&generate_conjugations("くる", "来る", "動詞", "カ行変格", 5000));
        for expected in [
            ("くる", "来る"),
            ("き", "来"),
            ("きて", "来て"),
            ("きた", "来た"),
            ("きます", "来ます"),
            ("こ", "来"),
            ("こない", "来ない"),
            ("くれ", "来れ"),
            ("くれば", "来れば"),
            ("こい", "来い"),
        ] {
            assert!(forms.contains(&(expected.0.to_string(), expected.1.to_string())));
        }
    }

    #[test]
    fn keiyoushi_conjugation_includes_expected_forms() {
        let forms = forms(&generate_conjugations("あかい", "赤い", "形容詞", "形容詞", 5000));
        for expected in [
            ("あかい", "赤い"),
            ("あかく", "赤く"),
            ("あかけれ", "赤けれ"),
            ("あかくて", "赤くて"),
            ("あかかった", "赤かった"),
            ("あかくない", "赤くない"),
            ("あかければ", "赤ければ"),
        ] {
            assert!(forms.contains(&(expected.0.to_string(), expected.1.to_string())));
        }
    }

    #[test]
    fn costs_match_expected_penalties() {
        let results = generate_conjugations("かく", "書く", "動詞", "五段-カ行", 5000);
        assert!(results.iter().any(|(r, s, c)| r == "かく" && s == "書く" && *c == 5000));
        assert!(results.iter().any(|(r, s, c)| r == "かき" && s == "書き" && *c == 5050));
        assert!(results.iter().any(|(r, s, c)| r == "かいて" && s == "書いて" && *c == 5100));
        assert!(results.iter().any(|(r, s, c)| r == "かかない" && s == "書かない" && *c == 5150));
        assert!(results.iter().any(|(r, s, c)| r == "かかれる" && s == "書かれる" && *c == 5200));
    }

    #[test]
    fn non_conjugatable_returns_base_only() {
        let results = generate_conjugations("にほん", "日本", "名詞", "*", 5000);
        assert_eq!(results, vec![("にほん".to_string(), "日本".to_string(), 5000)]);
    }

    #[test]
    fn skk_okurigana_expands_and_detects_entries() {
        assert!(is_skk_okurigana_entry("かk"));
        assert!(!is_skk_okurigana_entry("か"));
        assert!(!is_skk_okurigana_entry("かな"));

        let expanded = expand_skk_okurigana("かk", "書", 1);
        let forms = forms(&expanded);
        assert!(forms.contains(&("かく".to_string(), "書く".to_string())));
        assert!(forms.contains(&("かいて".to_string(), "書いて".to_string())));
        assert!(forms.contains(&("かかない".to_string(), "書かない".to_string())));
        assert!(expanded.iter().all(|(_, _, count)| *count == 1));
    }
}
