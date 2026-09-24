//! XML text helpers matching the Python exporter (nereus/asa.py, export.py,
//! xmljson.py) character for character, so both produce identical documents.

use chrono::{DateTime, Timelike, Utc};
use serde_json::Value;

/// Tethys indents with three spaces.
pub const IND: &str = "   ";

pub fn ind(depth: usize) -> String {
    IND.repeat(depth)
}

/// Python's xml.sax.saxutils.escape: & < > only.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Python's xml.sax.saxutils.quoteattr: escaped and quoted, preferring
/// double quotes.
pub fn quoteattr(s: &str) -> String {
    let mut data = escape(s).replace('\n', "&#10;").replace('\r', "&#13;").replace('\t', "&#9;");
    if data.contains('"') {
        if data.contains('\'') {
            data = format!("\"{}\"", data.replace('"', "&quot;"));
        } else {
            data = format!("'{data}'");
        }
    } else {
        data = format!("\"{data}\"");
    }
    data
}

/// Shortest text that reads back as the same number, as Python writes it
/// (15.0 -> "15", 0.1 -> "0.1", 1e-05 -> "1e-05").
pub fn format_num(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    py_repr(x)
}

/// Python's repr() of a float.
fn py_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    // Rust's {:e} gives the shortest round-trip digits, e.g. "-1.2345e-5".
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("exponent");
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if !(-4..16).contains(&exp) {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push_str(&format!("e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs()));
    } else if exp < 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-exp - 1) as usize));
        out.push_str(&digits);
    } else {
        let e = exp as usize;
        if digits.len() > e + 1 {
            out.push_str(&digits[..=e]);
            out.push('.');
            out.push_str(&digits[e + 1..]);
        } else {
            out.push_str(&digits);
            out.push_str(&"0".repeat(e + 1 - digits.len()));
            out.push_str(".0");
        }
    }
    out
}

/// xs:dateTime in UTC with millisecond precision (microseconds when needed).
pub fn format_time(t: &DateTime<Utc>) -> String {
    let us = t.nanosecond() / 1000 % 1_000_000;
    let base = t.format("%Y-%m-%dT%H:%M:%S");
    if !us.is_multiple_of(1000) {
        format!("{base}.{us:06}Z")
    } else {
        format!("{base}.{:03}Z", us / 1000)
    }
}

pub fn num_list(v: &[f64]) -> String {
    v.iter().map(|&x| format_num(x)).collect::<Vec<_>>().join(" ")
}

/// Element with text. `None` writes nothing; `Some("")` an empty element.
pub fn el(name: &str, value: Option<&str>, depth: usize, attrs: &[(&str, Option<String>)]) -> String {
    let Some(value) = value else { return String::new() };
    let a: String = attrs
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!(" {k}={}", quoteattr(v))))
        .collect();
    format!("{}<{name}{a}>{}</{name}>\n", ind(depth), escape(value))
}

/// Element that is always written.
pub fn el_s(name: &str, value: &str, depth: usize) -> String {
    el(name, Some(value), depth, &[])
}

/// Optional element (the Python `_opt`).
pub fn opt<T>(name: &str, value: Option<T>, depth: usize, conv: impl Fn(T) -> String) -> String {
    match value {
        None => String::new(),
        Some(v) => el(name, Some(&conv(v)), depth, &[]),
    }
}

pub fn opt_s(name: &str, value: Option<&str>, depth: usize) -> String {
    el(name, value, depth, &[])
}

/// Stored XML fragment on its own line.
pub fn frag(xml: Option<&str>, depth: usize) -> String {
    xml.map_or(String::new(), |x| format!("{}{x}\n", ind(depth)))
}

/// A jsonb block as XML; the original XML wins when one was kept.
pub fn block(name: &str, value: Option<&Value>, exact: Option<&str>, ns: Option<&str>, depth: usize) -> String {
    if exact.is_some() {
        return frag(exact, depth);
    }
    match value {
        None | Some(Value::Null) => String::new(),
        Some(v) => frag(Some(&block_to_xml(name, v, ns)), depth),
    }
}

// ------------------------------------------------------------ JSON -> XML
// Port of nereus/xmljson.py block_to_xml. Relies on serde_json's
// preserve_order so keys come out in the order PostgreSQL returns them,
// as they do for Python.

/// Element order the Tethys schema requires inside fixed-structure blocks.
fn schema_order(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "Description" => &["Objectives", "Abstract", "Method"],
        "QualityAssurance" => &["Description", "ResponsibleParty"],
        "MetadataInfo" => &["Contact", "Date", "UpdateFrequency"],
        "Contact" | "ResponsibleParty" => &["individualName", "organizationName", "positionName", "contactInfo"],
        "contactInfo" => &["phone", "address", "onlineResource", "hoursOfService", "contactInstructions"],
        "phone" => &["voice", "facsimile"],
        "address" => &[
            "deliveryPoint",
            "city",
            "administrativeArea",
            "postalCode",
            "country",
            "electronicMailAddress",
        ],
        "BespokeData" => &["Abstract", "Data", "UserDefined"],
        "Data" => &["URI", "Comment"],
        "SupportSoftware" => &["Software", "Version", "Parameters"],
        _ => return None,
    })
}

fn text(v: &Value) -> String {
    match v {
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                n.to_string()
            } else {
                format_num(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// The parent's namespace, or "none: always declare" at the top.
#[derive(Clone, Copy)]
enum Parent<'a> {
    Root,
    Ns(Option<&'a str>),
}

/// Standalone XML for element `name` (namespace `ns` unless the value says otherwise).
pub fn block_to_xml(name: &str, value: &Value, ns: Option<&str>) -> String {
    let mut out = String::new();
    write(name, value, ns, Parent::Root, &mut out);
    out
}

fn write(name: &str, value: &Value, ns: Option<&str>, parent: Parent, out: &mut String) {
    let obj = value.as_object();
    let ns: Option<&str> = match obj.and_then(|o| o.get("#ns")) {
        Some(v) => v.as_str().filter(|s| !s.is_empty()),
        None => ns,
    };
    let mut decls: Vec<(String, String)> = Vec::new();
    let declare = match parent {
        Parent::Root => true,
        Parent::Ns(p) => p != ns,
    };
    if declare {
        decls.push(("xmlns".into(), ns.unwrap_or("").into()));
    }
    let mut attrs = String::new();
    if let Some(o) = obj {
        let mut extra: Vec<String> = Vec::new();
        for (k, v) in o {
            let Some(k) = k.strip_prefix('@') else { continue };
            let mut k = k.to_string();
            if let Some(rest) = k.strip_prefix('{') {
                let (uri, local) = rest.split_once('}').unwrap_or((rest, ""));
                let prefix = match uri {
                    "http://www.w3.org/2001/XMLSchema-instance" => "xsi".to_string(),
                    "http://www.w3.org/XML/1998/namespace" => "xml".to_string(),
                    _ => match extra.iter().position(|u| u == uri) {
                        Some(i) => format!("a{i}"),
                        None => {
                            extra.push(uri.to_string());
                            format!("a{}", extra.len() - 1)
                        }
                    },
                };
                if prefix != "xml" {
                    let key = format!("xmlns:{prefix}");
                    match decls.iter_mut().find(|(k, _)| *k == key) {
                        Some(d) => d.1 = uri.to_string(),
                        None => decls.push((key, uri.to_string())),
                    }
                }
                k = format!("{prefix}:{local}");
            }
            attrs.push_str(&format!(" {k}={}", quoteattr(&text(v))));
        }
    }
    let head: String = std::iter::once(name.to_string())
        .chain(decls.iter().map(|(k, v)| format!(" {k}={}", quoteattr(v))))
        .collect::<String>()
        + &attrs;

    let Some(o) = obj else {
        let t = match value {
            Value::Null => String::new(),
            Value::String(s) if s.is_empty() => String::new(),
            v => escape(&text(v)),
        };
        if t.is_empty() {
            out.push_str(&format!("<{head}/>"));
        } else {
            out.push_str(&format!("<{head}>{t}</{name}>"));
        }
        return;
    };

    out.push('<');
    out.push_str(&head);
    out.push('>');
    if let Some(t) = o.get("#text") {
        out.push_str(&escape(&text(t)));
    }
    let keys: Vec<&String> = o.keys().filter(|k| !k.starts_with('@') && !k.starts_with('#')).collect();
    let expand = |ks: &[&String]| -> Vec<String> {
        ks.iter()
            .flat_map(|k| {
                let n = match &o[k.as_str()] {
                    Value::Array(a) => a.len(),
                    _ => 1,
                };
                std::iter::repeat_n((*k).clone(), n)
            })
            .collect()
    };
    let order: Vec<String> = match schema_order(name) {
        Some(known) if keys.iter().all(|k| known.contains(&k.as_str())) => {
            let mut ks = keys.clone();
            ks.sort_by_key(|k| known.iter().position(|x| x == k));
            expand(&ks)
        }
        _ => match o.get("#order").and_then(Value::as_array).filter(|a| !a.is_empty()) {
            Some(a) => a.iter().map(|v| v.as_str().unwrap_or_default().to_string()).collect(),
            None => expand(&keys),
        },
    };
    let mut taken: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for k in &order {
        let Some(v) = o.get(k) else { continue };
        let v = match v {
            Value::Array(a) => {
                let i = taken.entry(k.as_str()).or_insert(0);
                let item = a.get(*i);
                *i += 1;
                match item {
                    Some(x) => x,
                    None => continue,
                }
            }
            v => v,
        };
        write(k, v, ns, Parent::Ns(ns), out);
    }
    out.push_str(&format!("</{name}>"));
}

/// XML declaration and root start tag, with the root attributes kept at import.
pub fn root_open(name: &str, ns: Option<&str>, attrs: Option<&Value>) -> String {
    const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
    let mut root_attr = String::new();
    let mut need_xsi = false;
    if let Some(Value::Object(o)) = attrs {
        for (k, v) in o {
            let k = match k.strip_prefix(&format!("{{{XSI}}}")) {
                Some(local) => {
                    need_xsi = true;
                    format!("xsi:{local}")
                }
                None => k.clone(),
            };
            let v = v.as_str().map_or_else(|| text(v), str::to_string);
            root_attr.push_str(&format!(" {k}={}", quoteattr(&v)));
        }
    }
    let mut nsdecl = String::new();
    if let Some(ns) = ns.filter(|s| !s.is_empty()) {
        nsdecl.push_str(&format!(" xmlns=\"{ns}\""));
    }
    if need_xsi {
        nsdecl.push_str(&format!(" xmlns:xsi=\"{XSI}\""));
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<{name}{root_attr}{nsdecl}>\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_like_python() {
        assert_eq!(format_num(15.0), "15");
        assert_eq!(format_num(-0.0), "0");
        assert_eq!(format_num(0.1), "0.1");
        assert_eq!(format_num(1e-5), "1e-05");
        assert_eq!(format_num(0.0001), "0.0001");
        assert_eq!(format_num(123.456), "123.456");
        assert_eq!(format_num(1e16), "1e+16");
        assert_eq!(format_num(1e15), "1000000000000000.0");
        assert_eq!(format_num(1.5e20), "1.5e+20");
        assert_eq!(format_num(0.016666666666666666), "0.016666666666666666");
        assert_eq!(format_num(-2.5e-7), "-2.5e-07");
    }

    #[test]
    fn attributes_like_python() {
        assert_eq!(quoteattr("a"), "\"a\"");
        assert_eq!(quoteattr("a\"b"), "'a\"b'");
        assert_eq!(quoteattr("a\"b'c"), "\"a&quot;b'c\"");
        assert_eq!(quoteattr("x<y\n"), "\"x&lt;y&#10;\"");
    }

    #[test]
    fn json_blocks() {
        let v: Value = serde_json::from_str(
            r##"{"Threshold": {"@units": "dB", "#text": 12.5}, "MinICI_ms": 2, "Call": ["A", "B"], "#order": ["Threshold", "Call", "MinICI_ms", "Call"]}"##,
        )
        .unwrap();
        assert_eq!(
            block_to_xml("Parameters", &v, Some("urn:x")),
            "<Parameters xmlns=\"urn:x\"><Threshold units=\"dB\">12.5</Threshold><Call>A</Call>\
             <MinICI_ms>2</MinICI_ms><Call>B</Call></Parameters>"
        );
        assert_eq!(block_to_xml("Parameters", &Value::String(String::new()), None), "<Parameters xmlns=\"\"/>");
        let pam: Value = serde_json::from_str(r##"{"Setting": {"#ns": "", "#text": "x"}}"##).unwrap();
        assert_eq!(
            block_to_xml("P", &pam, Some("urn:t")),
            "<P xmlns=\"urn:t\"><Setting xmlns=\"\">x</Setting></P>"
        );
    }
}
