use crate::parser;
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt;

fn generate_object(
    object: &parser::Object,
    locales: &HashMap<String, Vec<parser::Object>>,
) -> String {
    let mut result = String::new();

    for (key, group) in &object
        .values
        .iter()
        .filter(|x| x.1.len() > 0)
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
                        .expect(&format!("unknown locale: {}", other_lang));
                    let other_object = other
                        .iter()
                        .find(|x| x.name == object.name)
                        .expect("could not find object to copy from");
                    result.push_str(generate_object(other_object, locales).as_str());
                }
                _ => panic!("only a string value is accepted for key \"copy\""),
            }
            continue;
        }

        if group.len() == 1 && group[0].len() == 0 {
            return result;
        } else if group.len() == 1 && group[0].len() == 1 {
            let singleton = &group[0][0];

            result.push_str(&match singleton {
                parser::Value::Raw(x) | parser::Value::String(x) => format!(
                    "        /// `{x:?}`\n        pub const {}: &'static str = {x:?};\n",
                    key,
                    x = x
                ),
                parser::Value::Integer(x) => format!(
                    "        /// `{x:?}`\n        pub const {}: i64 = {x:?};\n",
                    key,
                    x = x
                ),
            });
        } else if group.len() == 1 && group[0].iter().map(u8::from).all_equal() {
            let values = &group[0];
            let formatted = values.iter().map(|x| format!("{}", x)).join(", ");

            result.push_str(&match values[0] {
                parser::Value::Raw(_) | parser::Value::String(_) => format!(
                    "        /// `&[{x}]`\n        pub const {}: &'static [&'static str] = &[{x}];\n",
                    key,
                    x = formatted
                ),
                parser::Value::Integer(_) => format!(
                    "        /// `&[{x}]`\n        pub const {}: &'static [i64] = &[{}];\n",
                    key,
                    x = formatted
                ),
            });
        } else if group
            .iter()
            .map(|x| x.iter().map(u8::from))
            .flatten()
            .all_equal()
        {
            result.push_str("        /// ```ignore\n");
            result.push_str("        /// &[\n");
            for values in group.iter() {
                result.push_str("        ///     &[");
                result.push_str(&values.iter().map(|x| format!("{}", x)).join(", "));
                result.push_str("],\n");
            }
            result.push_str("        /// ]\n");
            result.push_str("        /// ```\n");

            result.push_str(&match group[0][0] {
                parser::Value::Raw(_) | parser::Value::String(_) => format!(
                    "        pub const {}: &'static [&'static [&'static str]] = &[",
                    key
                ),
                parser::Value::Integer(_) => {
                    format!("        pub const {}: &'static [&'static [i64]] = &[", key)
                }
            });
            for values in group.iter() {
                result.push_str("&[");
                result.push_str(&values.iter().map(|x| format!("{}", x)).join(", "));
                result.push_str("], ");
            }
            result.push_str("];\n");
        } else {
            unimplemented!("mixed types");
        }
    }

    result
}

fn generate_locale(
    lang_normalized: &str,
    objects: &Vec<parser::Object>,
    locales: &HashMap<String, Vec<parser::Object>>,
) -> String {
    let mut result = String::new();

    result.push_str("#[allow(non_snake_case,non_camel_case_types,dead_code,unused_imports)]\n");
    result.push_str(&format!("pub mod {} {{\n", lang_normalized));

    for object in objects.iter() {
        if object.name == "LC_COLLATE"
            || object.name == "LC_CTYPE"
            || object.name == "LC_MEASUREMENT"
            || object.name == "LC_PAPER"
            || object.name == "LC_NAME"
        {
            continue;
        } else if object.values.len() == 1 {
            let (key, value) = &object.values[0];
            match key.as_str() {
                "copy" => {
                    assert_eq!(value.len(), 1);
                    match &value[0] {
                        parser::Value::String(x) => {
                            result.push_str(&format!(
                                "    pub use super::{}::{};\n",
                                x.replace("@", "_"),
                                object.name
                            ));
                        }
                        x => panic!("unexpected value for key {}: {:?}", key, x),
                    }
                }
                _ => {}
            }
        } else {
            result.push_str(&format!("    pub mod {} {{\n", object.name));
            result.push_str(generate_object(&object, locales).as_str());
            result.push_str("    }\n\n");
        }
    }

    result.push_str("}\n\n");

    result
}

fn generate_variants(langs: &[(&str, &str)]) -> String {
    let mut result = String::new();

    result.push_str("#[allow(non_camel_case_types,dead_code)]\n");
    result.push_str("#[derive(Debug, Copy, Clone, PartialEq)]\n");
    result.push_str("pub enum Locale {\n");
    for (lang, norm) in langs {
        result.push_str(&format!("    /// {}\n", lang));
        result.push_str(&format!("    {},\n", norm));
    }
    result.push_str("}\n\n");

    result.push_str("impl core::convert::TryFrom<&str> for Locale {\n");
    result.push_str("    type Error = UnknownLocale;\n\n");
    result.push_str("    fn try_from(i: &str) -> Result<Self, Self::Error> {\n");
    result.push_str("        match i {\n");
    for (lang, norm) in langs {
        result.push_str(&format!(
            "            {:?} => Ok(Locale::{}),\n",
            lang, norm,
        ));
    }
    result.push_str("            _ => Err(UnknownLocale),\n");
    result.push_str("        }\n");
    result.push_str("    }\n");
    result.push_str("}\n\n");

    result.push_str("#[macro_export]\n");
    result.push_str("macro_rules! locale_match {\n");
    result.push_str("    ($locale:expr => $($item:ident)::+) => {{\n");
    result.push_str("        match $locale {\n");
    for (_, norm) in langs {
        result.push_str(&format!(
            "            $crate::Locale::{} => $crate::{}::$($item)::+,\n",
            norm, norm,
        ));
    }
    result.push_str("        }\n");
    result.push_str("    }}\n");
    result.push_str("}\n\n");

    result
}

pub struct CodeGenerator(pub HashMap<String, Vec<parser::Object>>);

impl fmt::Display for CodeGenerator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
            let code = generate_locale(normalized[lang].as_str(), &objects, locales);
            write!(f, "{}", code)?;
        }

        let mut sorted: Vec<_> = locales
            .iter()
            .map(|(lang, _)| (lang.as_str(), normalized[lang].as_str()))
            .collect();
        sorted.sort_unstable_by_key(|(lang, _)| lang.to_string());
        write!(f, "{}", generate_variants(&sorted),)
    }
}
