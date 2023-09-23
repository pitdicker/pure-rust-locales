use crate::parser;
use indenter::CodeFormatter;
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Write;

fn generate_object<W: Write>(
    f: &mut CodeFormatter<W>,
    object: &parser::Object,
    locales: &HashMap<String, Vec<parser::Object>>,
) -> fmt::Result {
    for (key, group) in &object
        .values
        .iter()
        .filter(|x| !x.1.is_empty())
        .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
        .group_by(|x| x.0.clone())
    {
        let key = key
            .replace("'", "")
            .replace("\"", "")
            .replace("-", "_")
            .replace("=", "eq")
            .replace("<", "lt")
            .replace("..", "dotdot")
            .replace("2", "two")
            .to_uppercase();
        let group: Vec<_> = group.map(|x| &x.1).collect();

        if key == "copy" || key == "include" {
            match &group[0][0] {
                parser::Value::String(other_lang) => {
                    let other = locales
                        .get(other_lang)
                        .unwrap_or_else(|| panic!("unknown locale: {}", other_lang));
                    let other_object = other
                        .iter()
                        .find(|x| x.name == object.name)
                        .expect("could not find object to copy from");
                    generate_object(f, other_object, locales)?;
                }
                _ => panic!("only a string value is accepted for key \"copy\""),
            }
            continue;
        }

        if group.len() == 1 && group[0].is_empty() {
            return Ok(());
        } else if group.len() == 1 && group[0].len() == 1 {
            let singleton = &group[0][0];

            match singleton {
                parser::Value::Raw(x) | parser::Value::String(x) => write!(
                    f,
                    r#"
                    /// `{x:?}`
                    pub const {key}: &str = {x:?};
                    "#,
                    key = key,
                    x = x
                )?,
                parser::Value::Integer(x) => write!(
                    f,
                    r#"
                    /// `{x:?}`
                    pub const {key}: i64 = {x:?};
                    "#,
                    key = key,
                    x = x
                )?,
            }
        } else if group.len() == 1 && group[0].iter().map(u8::from).all_equal() {
            let values = &group[0];
            let formatted = values.iter().map(|x| format!("{}", x)).join(", ");

            match &values[0] {
                parser::Value::Raw(_) | parser::Value::String(_) => write!(
                    f,
                    r#"
                    /// `&[{x}]`
                    pub const {key}: &[&str] = &[{x}];
                    "#,
                    key = key,
                    x = formatted
                )?,
                parser::Value::Integer(_) => write!(
                    f,
                    r#"
                    /// `&[{x}]`
                    pub const {key}: &[i64] = &[{x}];
                    "#,
                    key = key,
                    x = formatted
                )?,
            }
        } else if group
            .iter()
            .map(|x| x.iter().map(u8::from))
            .flatten()
            .all_equal()
        {
            write!(
                f,
                r#"
                /// ```ignore
                /// &[
                "#,
            )?;

            for values in group.iter() {
                write!(
                    f,
                    r#"
                    ///     &[{}],
                    "#,
                    values.iter().map(|x| format!("{}", x)).join(", "),
                )?;
            }

            write!(
                f,
                r#"
                /// ]
                /// ```
                "#,
            )?;

            match group[0][0] {
                parser::Value::Raw(_) | parser::Value::String(_) => write!(
                    f,
                    r#"
                    pub const {}: &[&[&str]] = &[
                    "#,
                    key
                )?,
                parser::Value::Integer(_) => write!(
                    f,
                    r#"
                    pub const {}: &[&[i64]] = &[
                    "#,
                    key,
                )?,
            }
            f.indent(1);

            for values in group.iter() {
                write!(
                    f,
                    r#"
                    &[{}],
                    "#,
                    values.iter().map(|x| format!("{}", x)).join(", "),
                )?;
            }

            f.dedent(1);
            write!(
                f,
                r#"
                ];
                "#,
            )?;
        } else {
            unimplemented!("mixed types");
        }
    }

    Ok(())
}

fn generate_locale<W: Write>(
    f: &mut CodeFormatter<W>,
    lang_normalized: &str,
    objects: &[parser::Object],
    locales: &HashMap<String, Vec<parser::Object>>,
) -> fmt::Result {
    write!(
        f,
        r#"

        #[allow(non_snake_case,non_camel_case_types,dead_code,unused_imports)]
        pub mod {} {{
        "#,
        lang_normalized,
    )?;
    f.indent(1);

    for object in objects.iter().sorted_by_key(|x| x.name.to_string()) {
        if object.name == "LC_COLLATE"
            || object.name == "LC_CTYPE"
            || object.name == "LC_MEASUREMENT"
            || object.name == "LC_PAPER"
            || object.name == "LC_NAME"
        {
            continue;
        } else if object.values.len() == 1 {
            let (key, value) = &object.values[0];
            #[allow(clippy::single_match)]
            match key.as_str() {
                "copy" => {
                    assert_eq!(value.len(), 1);
                    match &value[0] {
                        parser::Value::String(x) => write!(
                            f,
                            r#"
                            pub use super::{}::{};
                            "#,
                            x.replace("@", "_"),
                            object.name,
                        )?,
                        x => panic!("unexpected value for key {}: {:?}", key, x),
                    }
                }
                _ => {}
            }
        } else {
            write!(
                f,
                r#"
                pub mod {} {{
                "#,
                object.name,
            )?;
            f.indent(1);
            generate_object(f, &object, locales)?;
            f.dedent(1);
            write!(
                f,
                r#"
                }}
                "#,
            )?;
        }
    }

    f.dedent(1);
    write!(
        f,
        r#"
        }}
        "#,
    )
}

fn generate_variants<W: Write>(
    f: &mut CodeFormatter<W>,
    langs: &[(&str, &str, String)],
) -> fmt::Result {
    write!(
        f,
        r#"

        /// Locales matching the locales in `glibc`.
        ///
        /// Most locale names follow the syntax `language[_territory][@modifier]`.
        /// The `@` is replaced with `_` in the `enum` variant names.
        ///
        /// The default locale is `POSIX`.
        ///
        /// The Free Software Foundation does not claim any copyright interest in the locale data of the
        /// GNU C Library; they believe it is not copyrightable.
        #[allow(non_camel_case_types,dead_code)]
        #[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
        pub enum Locale {{
        "#,
    )?;
    f.indent(1);

    for (lang, norm, desc) in langs {
        write!(f, "\n/// `{}`: {}\n", lang, desc)?;
        if lang == &"POSIX" {
            writeln!(f, "\n#[default]\n")?;
        }
        writeln!(f, "\n{},\n", norm)?;
    }

    f.dedent(1);
    write!(
        f,
        r#"
        }}

        impl core::convert::TryFrom<&str> for Locale {{
            type Error = UnknownLocale;

            fn try_from(i: &str) -> Result<Self, Self::Error> {{
                match i {{
        "#,
    )?;
    f.indent(3);

    for (lang, norm, _) in langs {
        write!(
            f,
            r#"
            {lang:?} => Ok(Locale::{norm}),
            "#,
            lang = lang,
            norm = norm,
        )?;
    }

    f.dedent(3);
    write!(
        f,
        r#"
                    _ => Err(UnknownLocale),
                }}
            }}
        }}

        impl core::fmt::Display for Locale {{
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
                f.write_str(match self {{
        "#,
    )?;
    f.indent(3);

    for (lang, norm, _) in langs {
        write!(
            f,
            r#"
            Locale::{norm} => {lang:?},
            "#,
            lang = lang,
            norm = norm,
        )?;
    }

    f.dedent(3);
    write!(
        f,
        r#"
                }})
            }}
        }}

        impl core::fmt::Debug for Locale {{
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
                core::fmt::Display::fmt(self, f)
            }}
        }}

        #[macro_export]
        macro_rules! locale_match {{
            ($locale:expr => $($item:ident)::+) => {{{{
                match $locale {{
        "#,
    )?;
    f.indent(3);

    for (_, norm, _) in langs {
        write!(
            f,
            r#"
            $crate::Locale::{norm} => $crate::{norm}::$($item)::+,
            "#,
            norm = norm,
        )?;
    }
    f.dedent(3);

    write!(
        f,
        r#"
                }}
            }}}}
        }}

        "#,
    )
}

pub struct CodeGenerator(pub HashMap<String, Vec<parser::Object>>);

impl fmt::Display for CodeGenerator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut f = CodeFormatter::new(f, "    ");
        write!(
            f,
            r#"
            #![no_std]

            #[derive(Debug)]
            pub struct UnknownLocale;

            "#,
        )?;

        let locales = &self.0;

        let normalized: HashMap<_, _> = locales
            .iter()
            .map(|(lang, _)| (lang, lang.replace("@", "_")))
            .collect();

        let mut sorted: Vec<_> = locales.iter().collect();
        sorted.sort_unstable_by_key(|(lang, _)| lang.to_string());
        for (lang, objects) in sorted.iter() {
            generate_locale(&mut f, normalized[lang].as_str(), &objects, locales)?;
        }

        let mut sorted: Vec<_> = locales
            .iter()
            .map(|(lang, objects)| {
                let mut desc = None;
                if let Some(object) = objects.iter().find(|o| o.name == "LC_IDENTIFICATION") {
                    for (key, values) in object.values.iter() {
                        if key == "title" {
                            if let Some(crate::Value::String(title)) = values.get(0) {
                                let mut title = title.clone();
                                if !title.ends_with('.') {
                                    title.push('.');
                                }
                                desc = Some(title);
                            }
                            break;
                        }
                    }
                }
                let desc = desc.unwrap_or_else(|| match lang == "POSIX" {
                    true => "POSIX Standard Locale.".to_string(),
                    false => "".to_string(),
                });
                (lang.as_str(), normalized[lang].as_str(), desc)
            })
            .collect();
        sorted.sort_unstable_by_key(|(lang, _, _)| lang.to_string());
        generate_variants(&mut f, &sorted)
    }
}
