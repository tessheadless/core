// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::BTreeMap;

use super::*;
use crate::{Translation, TranslationKey, TranslationState};

fn key(name: &str, plural: bool, texts: &[(&str, &str)]) -> TranslationKey {
    TranslationKey {
        name: name.into(),
        namespace: String::new(),
        description: Some("Shown on the home page".into()),
        tags: vec![],
        plural,
        translations: texts
            .iter()
            .map(|(l, t)| {
                let tr = Translation {
                    text: t.to_string(),
                    state: TranslationState::Translated,
                    updated_at: String::new(),
                    updated_by: String::new(),
                };
                (l.to_string(), tr)
            })
            .collect::<BTreeMap<_, _>>(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn languages() -> Vec<Language> {
    vec![
        Language { tag: "en".into(), name: "English".into(), base: true },
        Language { tag: "fr-CA".into(), name: "Français".into(), base: false },
    ]
}

fn sample() -> Vec<TranslationKey> {
    vec![
        key("home.title", false, &[("en", "Welcome, {name}!"), ("fr-CA", "Bienvenue, {name} !")]),
        key("tricky", false, &[("en", "Quotes \" ' & <b> = : # \\ é 🎉\nline 2"), ("fr-CA", "Déjà vu")]),
        key("items", true, &[("en", "{count, plural, one {# item} other {# items}}"), ("fr-CA", "{count, plural, one {# article} other {# articles}}")]),
    ]
}

fn texts(keys: &[ImportedKey], lang: &str) -> BTreeMap<String, String> {
    keys.iter()
        .filter_map(|k| k.translations.iter().find(|(l, _)| l == lang).map(|(_, t)| (k.name.clone(), t.clone())))
        .collect()
}

#[test]
fn every_importable_format_round_trips() {
    let keys = sample();
    for format in Format::ALL.into_iter().filter(|f| f.importable()) {
        let files = export(&keys, &languages(), &ExportOptions::new(format)).unwrap();
        let file = files.iter().find(|f| format.multi_language() || f.path.contains("fr")).unwrap();
        let lang = (!format.multi_language()).then_some("fr-CA");
        let parsed = parse(format, &file.content, lang).unwrap_or_else(|e| panic!("{format:?}: {e}\n{}", file.content));
        let got = texts(&parsed, "fr-CA");
        for k in &keys {
            // Android resource names cannot hold dots.
            let name = if format == Format::AndroidXml { write::android_name(&k.name) } else { k.name.clone() };
            assert_eq!(got.get(&name).map(String::as_str), k.text("fr-CA"), "{format:?} {name}\n{}", file.content);
        }
        if format.multi_language() {
            assert_eq!(texts(&parsed, "en").get("tricky").map(String::as_str), keys[1].text("en"), "{format:?}");
        }
    }
}

#[test]
fn english_special_characters_survive_every_format() {
    let keys = sample();
    for format in Format::ALL.into_iter().filter(|f| f.importable() && !f.multi_language()) {
        let files = export(&keys, &languages(), &ExportOptions { languages: vec!["en".into()], ..ExportOptions::new(format) }).unwrap();
        let parsed = parse(format, &files[0].content, Some("en")).unwrap();
        let name = if format == Format::AndroidXml { "tricky".to_string() } else { "tricky".into() };
        assert_eq!(texts(&parsed, "en").get(&name).map(String::as_str), keys[1].text("en"), "{format:?}\n{}", files[0].content);
    }
}

#[test]
fn i18next_plurals_and_layout() {
    let files = export(&sample(), &languages(), &ExportOptions::new(Format::I18next)).unwrap();
    let en = files.iter().find(|f| f.path == "en/translation.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&en.content).unwrap();
    assert_eq!(v["items_one"], "{{count}} item");
    assert_eq!(v["items_other"], "{{count}} items");
    assert_eq!(v["home"]["title"], "Welcome, {{name}}!");
    let parsed = parse(Format::I18next, &en.content, Some("en")).unwrap();
    let items = parsed.iter().find(|k| k.name == "items").unwrap();
    assert!(items.plural);
    assert_eq!(items.translations[0].1, "{count, plural, one {# item} other {# items}}");
}

#[test]
fn nested_json_keeps_conflicting_keys() {
    let keys = vec![key("a", false, &[("en", "A")]), key("a.b", false, &[("en", "AB")])];
    let opts = ExportOptions { languages: vec!["en".into()], ..ExportOptions::new(Format::JsonNested) };
    let files = export(&keys, &languages(), &opts).unwrap();
    let parsed = parse(Format::JsonNested, &files[0].content, Some("en")).unwrap();
    let got = texts(&parsed, "en");
    assert_eq!(got.get("a").map(String::as_str), Some("A"));
    assert_eq!(got.get("a.b").map(String::as_str), Some("AB"));
}

#[test]
fn paths_states_fallback_and_android_plurals() {
    let mut keys = sample();
    keys[0].namespace = "common".into();
    keys[1].translations.remove("fr-CA");
    let opts = ExportOptions { fallback_to_base: true, ..ExportOptions::new(Format::AndroidXml) };
    let files = export(&keys, &languages(), &opts).unwrap();
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["values-en/strings.xml", "values-fr-rCA/strings.xml", "common/values-en/strings.xml", "common/values-fr-rCA/strings.xml"]);
    let fr = &files[1].content;
    assert!(fr.contains("<item quantity=\"one\">%d article</item>"), "{fr}");
    assert!(fr.contains("Quotes"), "fallback to the base text");

    let reviewed_only = ExportOptions { states: vec![TranslationState::Reviewed], ..ExportOptions::new(Format::Json) };
    let files = export(&keys, &languages(), &reviewed_only).unwrap();
    assert!(files.iter().all(|f| f.content.trim() == "{}"));
    assert_eq!(android_qualifier("zh-Hant-TW"), "b+zh+Hant+TW");
    assert!(zip_files(&files).unwrap().starts_with(b"PK"));
}

#[test]
fn imports_hand_written_files() {
    let po = "msgid \"\"\nmsgstr \"\"\n\"Language: de\\n\"\n\n#. Greeting\nmsgid \"hello\"\nmsgstr \"Hal\"\n\"lo\"\n";
    let k = parse(Format::Po, po, None).unwrap();
    assert_eq!(k[0].translations, vec![("de".to_string(), "Hallo".to_string())]);
    assert_eq!(k[0].description.as_deref(), Some("Greeting"));

    let props = "# c\nlong.key = first \\\n   second\nemoji=\\ud83c\\udf89\n";
    let k = parse(Format::Properties, props, Some("en")).unwrap();
    assert_eq!(texts(&k, "en")["long.key"], "first second");
    assert_eq!(texts(&k, "en")["emoji"], "🎉");

    let xliff2 = r#"<xliff version="2.0" srcLang="en" trgLang="it"><file id="f"><unit id="bye"><segment><source>Bye</source><target>Ciao &amp; addio</target></segment></unit></file></xliff>"#;
    let k = parse(Format::Xliff, xliff2, None).unwrap();
    assert_eq!(texts(&k, "it")["bye"], "Ciao & addio");
    assert_eq!(texts(&k, "en")["bye"], "Bye");

    let strings = "/* c */\n\"a\" = \"x \\\"y\\\"\";\n// line\n\"b\"=\"z\";";
    let k = parse(Format::AppleStrings, strings, Some("en")).unwrap();
    assert_eq!(texts(&k, "en")["a"], "x \"y\"");
    assert!(parse(Format::Json, "[]", Some("en")).is_err());
    assert!(parse(Format::Json, "{}", None).is_err(), "language required");
    assert!(parse(Format::Yaml, "a: b", Some("en")).is_err());
}
