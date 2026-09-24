//! R workspace (.RData) writer for data frames, readable with `load()`.
//!
//! This is R's serialization format, version 3, XDR (big-endian), gzipped,
//! behind the "RDX3" header. Only what data frames need is written: numeric,
//! character, logical and POSIXct columns, and a named character vector.
//! Format reference: R Internals, "Serialization Formats", and
//! src/main/serialize.c in the R sources.

use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::table::{Column, Table};

const NILVALUE_SXP: u32 = 254;
const SYMSXP: u32 = 1;
const LISTSXP: u32 = 2;
const CHARSXP: u32 = 9;
const LGLSXP: u32 = 10;
const INTSXP: u32 = 13;
const REALSXP: u32 = 14;
const STRSXP: u32 = 16;
const VECSXP: u32 = 19;

const IS_OBJECT: u32 = 1 << 8;
const HAS_ATTR: u32 = 1 << 9;
const HAS_TAG: u32 = 1 << 10;
// CHARSXP encoding flags, stored in the "levels" bits (12 and up).
const UTF8_MASK: u32 = 1 << 3;
const ASCII_MASK: u32 = 1 << 6;

const NA_INTEGER: i32 = i32::MIN;
/// R's NA_real_: a NaN with payload 1954.
const NA_REAL: u64 = 0x7FF0_0000_0000_07A2;

/// An R object to save.
pub enum RObject<'a> {
    DataFrame(&'a Table),
    /// A named character vector.
    Named(Vec<(String, String)>),
}

struct Out<W: Write> {
    w: W,
}

impl<W: Write> Out<W> {
    fn int(&mut self, x: i32) -> std::io::Result<()> {
        self.w.write_all(&x.to_be_bytes())
    }
    fn flags(&mut self, f: u32) -> std::io::Result<()> {
        self.w.write_all(&f.to_be_bytes())
    }
    fn len(&mut self, n: usize) -> std::io::Result<()> {
        self.int(i32::try_from(n).map_err(|_| std::io::Error::other("vector too long for R"))?)
    }
    fn real(&mut self, x: f64) -> std::io::Result<()> {
        let bits = if x.is_nan() { NA_REAL } else { x.to_bits() };
        self.w.write_all(&bits.to_be_bytes())
    }

    fn charsxp(&mut self, s: Option<&str>) -> std::io::Result<()> {
        match s {
            None => {
                self.flags(CHARSXP)?;
                self.int(-1)
            }
            Some(s) => {
                let enc = if s.is_ascii() { ASCII_MASK } else { UTF8_MASK };
                self.flags(CHARSXP | enc << 12)?;
                self.len(s.len())?;
                self.w.write_all(s.as_bytes())
            }
        }
    }

    fn symbol(&mut self, name: &str) -> std::io::Result<()> {
        // Always written in full; R accepts a repeated symbol (it only
        // uses back-references to save space).
        self.flags(SYMSXP)?;
        self.charsxp(Some(name))
    }

    fn strsxp<'s>(&mut self, attr: u32, v: impl ExactSizeIterator<Item = Option<&'s str>>) -> std::io::Result<()> {
        self.flags(STRSXP | attr)?;
        self.len(v.len())?;
        for s in v {
            self.charsxp(s)?;
        }
        Ok(())
    }

    fn strings(&mut self, v: &[&str]) -> std::io::Result<()> {
        self.strsxp(0, v.iter().map(|s| Some(*s)))
    }

    /// An attribute pairlist entry: tag, then the caller writes the value.
    fn attr(&mut self, name: &str) -> std::io::Result<()> {
        self.flags(LISTSXP | HAS_TAG)?;
        self.symbol(name)
    }

    fn column(&mut self, c: &Column) -> std::io::Result<()> {
        match c {
            Column::Num(v) => {
                self.flags(REALSXP)?;
                self.len(v.len())?;
                v.iter().try_for_each(|&x| self.real(x))
            }
            Column::Time(v) => {
                self.flags(REALSXP | IS_OBJECT | HAS_ATTR)?;
                self.len(v.len())?;
                v.iter().try_for_each(|&x| self.real(x))?;
                self.attr("class")?;
                self.strings(&["POSIXct", "POSIXt"])?;
                self.attr("tzone")?;
                self.strings(&["UTC"])?;
                self.flags(NILVALUE_SXP)
            }
            Column::Str(v) => self.strsxp(0, v.iter().map(|s| s.as_deref())),
            Column::Bool(v) => {
                self.flags(LGLSXP)?;
                self.len(v.len())?;
                v.iter().try_for_each(|b| self.int(b.map_or(NA_INTEGER, i32::from)))
            }
        }
    }

    fn data_frame(&mut self, t: &Table) -> std::io::Result<()> {
        self.flags(VECSXP | IS_OBJECT | HAS_ATTR)?;
        self.len(t.columns.len())?;
        for c in &t.columns {
            self.column(c)?;
        }
        self.attr("names")?;
        self.strsxp(0, t.names.iter().map(|s| Some(s.as_str())))?;
        self.attr("class")?;
        self.strings(&["data.frame"])?;
        // Compact row names: c(NA, -n) means 1..n.
        self.attr("row.names")?;
        self.flags(INTSXP)?;
        self.len(2)?;
        self.int(NA_INTEGER)?;
        self.int(-(i32::try_from(t.n_rows()).map_err(|_| std::io::Error::other("too many rows for R"))?))?;
        self.flags(NILVALUE_SXP)
    }

    fn named(&mut self, v: &[(String, String)]) -> std::io::Result<()> {
        self.strsxp(HAS_ATTR, v.iter().map(|(_, s)| Some(s.as_str())))?;
        self.attr("names")?;
        self.strsxp(0, v.iter().map(|(k, _)| Some(k.as_str())))?;
        self.flags(NILVALUE_SXP)
    }
}

/// Write objects, by name, as an .RData file.
pub fn write<W: Write>(w: W, objects: &[(&str, RObject)]) -> std::io::Result<()> {
    let mut out = Out { w: GzEncoder::new(w, Compression::new(6)) };
    out.w.write_all(b"RDX3\nX\n")?;
    out.int(3)?; // format version
    out.int(4 << 16 | 3 << 8)?; // written by R 4.3.0 (for R's information only)
    out.int(3 << 16 | 5 << 8)?; // readable from R 3.5.0
    out.len(5)?;
    out.w.write_all(b"UTF-8")?;
    for (name, obj) in objects {
        out.attr(name)?;
        match obj {
            RObject::DataFrame(t) => out.data_frame(t)?,
            RObject::Named(v) => out.named(v)?,
        }
    }
    out.flags(NILVALUE_SXP)?;
    out.w.finish()?.flush()
}
