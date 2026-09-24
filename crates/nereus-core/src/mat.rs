//! MATLAB MAT-file (Level 5) writer: doubles, logicals, char, cell arrays
//! and structs, each variable zlib-compressed. Readable by MATLAB 7 and
//! later, Octave, and scipy.io.loadmat.
//!
//! Format reference: "MATLAB 7 MAT-File Format" (MathWorks). Level 5 limits
//! each variable to 2 GB uncompressed, far above an export of detections.

use std::io::{Seek, SeekFrom, Write};

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::table::{Column, Table};

// Data types
const MI_INT8: u32 = 1;
const MI_UINT8: u32 = 2;
const MI_UINT16: u32 = 4;
const MI_INT32: u32 = 5;
const MI_UINT32: u32 = 6;
const MI_DOUBLE: u32 = 9;
const MI_MATRIX: u32 = 14;
const MI_COMPRESSED: u32 = 15;
// Array classes
const MX_CELL: u8 = 1;
const MX_STRUCT: u8 = 2;
const MX_CHAR: u8 = 4;
const MX_DOUBLE: u8 = 6;
const MX_UINT8: u8 = 9;
const FLAG_LOGICAL: u8 = 0x02;
/// Bytes per struct field name, including the terminating NUL (MATLAB's
/// namelengthmax is 63).
const FIELD_LEN: usize = 64;

/// Days from 0000-01-00 (MATLAB datenum 0) to 1970-01-01.
pub const DATENUM_1970: f64 = 719_529.0;

pub fn datenum(unix_s: f64) -> f64 {
    unix_s / 86_400.0 + DATENUM_1970
}

#[derive(Clone, Debug)]
pub enum MatValue {
    Double { rows: usize, cols: usize, data: Vec<f64> },
    Logical { rows: usize, cols: usize, data: Vec<u8> },
    /// A row of text; empty text is a 0x0 char array, as MATLAB writes ''.
    Char(String),
    Cell { rows: usize, cols: usize, items: Vec<MatValue> },
    /// items[element][field], elements in column-major order.
    Struct { rows: usize, cols: usize, fields: Vec<String>, items: Vec<Vec<MatValue>> },
}

impl MatValue {
    pub fn scalar(x: f64) -> Self {
        MatValue::Double { rows: 1, cols: 1, data: vec![x] }
    }

    pub fn text(s: impl Into<String>) -> Self {
        MatValue::Char(s.into())
    }

    pub fn column(data: Vec<f64>) -> Self {
        MatValue::Double { rows: data.len(), cols: 1, data }
    }

    /// A 1x1 struct from (name, value) pairs.
    pub fn record(fields: Vec<(&str, MatValue)>) -> Self {
        let (names, values): (Vec<_>, Vec<_>) = fields.into_iter().map(|(n, v)| (n.to_string(), v)).unzip();
        MatValue::Struct { rows: 1, cols: 1, fields: names, items: vec![values] }
    }

    /// A 1xN struct array; every element has the same fields.
    pub fn struct_array(fields: Vec<String>, items: Vec<Vec<MatValue>>) -> Self {
        MatValue::Struct { rows: 1, cols: items.len(), fields, items }
    }

    /// A table as a 1x1 struct of Nx1 columns: numbers as double, times as
    /// datenum, text as a cell array of char, logicals as logical (missing = false).
    pub fn columns(t: &Table) -> Self {
        let n = t.n_rows();
        let values = t
            .columns
            .iter()
            .map(|c| match c {
                Column::Num(v) => MatValue::column(v.clone()),
                Column::Time(v) => MatValue::column(v.iter().map(|&x| datenum(x)).collect()),
                Column::Str(v) => MatValue::Cell {
                    rows: n,
                    cols: 1,
                    items: v.iter().map(|s| MatValue::Char(s.clone().unwrap_or_default())).collect(),
                },
                Column::Bool(v) => MatValue::Logical {
                    rows: n,
                    cols: 1,
                    data: v.iter().map(|b| u8::from(b.unwrap_or(false))).collect(),
                },
            })
            .collect();
        MatValue::Struct { rows: 1, cols: 1, fields: t.names.clone(), items: vec![values] }
    }
}

fn pad8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

/// Bytes of a miMATRIX element after its 8-byte tag.
fn body_size(v: &MatValue, name: &str) -> usize {
    let head = 16 /* array flags */ + 8 + 8 /* 2 dims */ + 8 + pad8(name.len());
    head + match v {
        MatValue::Double { data, .. } => 8 + data.len() * 8,
        MatValue::Logical { data, .. } => 8 + pad8(data.len()),
        MatValue::Char(s) => 8 + pad8(s.encode_utf16().count() * 2),
        MatValue::Cell { items, .. } => items.iter().map(|i| 8 + body_size(i, "")).sum(),
        MatValue::Struct { fields, items, .. } => {
            8 + 8 + pad8(fields.len() * FIELD_LEN)
                + items.iter().flatten().map(|i| 8 + body_size(i, "")).sum::<usize>()
        }
    }
}

struct Out<W: Write> {
    w: W,
}

impl<W: Write> Out<W> {
    fn u32(&mut self, x: u32) -> std::io::Result<()> {
        self.w.write_all(&x.to_le_bytes())
    }
    fn tag(&mut self, ty: u32, n: usize) -> std::io::Result<()> {
        self.u32(ty)?;
        self.u32(u32::try_from(n).map_err(|_| too_big())?)
    }
    fn pad(&mut self, n: usize) -> std::io::Result<()> {
        self.w.write_all(&[0u8; 8][..pad8(n) - n])
    }

    fn matrix(&mut self, v: &MatValue, name: &str) -> std::io::Result<()> {
        self.tag(MI_MATRIX, body_size(v, name))?;
        let (class, flags, rows, cols) = match v {
            MatValue::Double { rows, cols, .. } => (MX_DOUBLE, 0, *rows, *cols),
            MatValue::Logical { rows, cols, .. } => (MX_UINT8, FLAG_LOGICAL, *rows, *cols),
            MatValue::Char(s) => {
                let n = s.encode_utf16().count();
                (MX_CHAR, 0, usize::from(n > 0), n)
            }
            MatValue::Cell { rows, cols, .. } => (MX_CELL, 0, *rows, *cols),
            MatValue::Struct { rows, cols, .. } => (MX_STRUCT, 0, *rows, *cols),
        };
        self.tag(MI_UINT32, 8)?;
        self.u32(class as u32 | (flags as u32) << 8)?;
        self.u32(0)?;
        self.tag(MI_INT32, 8)?;
        for d in [rows, cols] {
            self.w.write_all(&(i32::try_from(d).map_err(|_| too_big())?).to_le_bytes())?;
        }
        self.tag(MI_INT8, name.len())?;
        self.w.write_all(name.as_bytes())?;
        self.pad(name.len())?;

        match v {
            MatValue::Double { data, .. } => {
                self.tag(MI_DOUBLE, data.len() * 8)?;
                for x in data {
                    self.w.write_all(&x.to_le_bytes())?;
                }
            }
            MatValue::Logical { data, .. } => {
                self.tag(MI_UINT8, data.len())?;
                self.w.write_all(data)?;
                self.pad(data.len())?;
            }
            MatValue::Char(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                self.tag(MI_UINT16, units.len() * 2)?;
                for u in &units {
                    self.w.write_all(&u.to_le_bytes())?;
                }
                self.pad(units.len() * 2)?;
            }
            MatValue::Cell { items, .. } => {
                for i in items {
                    self.matrix(i, "")?;
                }
            }
            MatValue::Struct { fields, items, .. } => {
                // Field name length, in the small (packed) element format.
                self.u32(4 << 16 | MI_INT32)?;
                self.w.write_all(&(FIELD_LEN as i32).to_le_bytes())?;
                self.tag(MI_INT8, fields.len() * FIELD_LEN)?;
                for f in fields {
                    let mut buf = [0u8; FIELD_LEN];
                    let b = f.as_bytes();
                    let n = b.len().min(FIELD_LEN - 1);
                    buf[..n].copy_from_slice(&b[..n]);
                    self.w.write_all(&buf)?;
                }
                self.pad(fields.len() * FIELD_LEN)?;
                for element in items {
                    for i in element {
                        self.matrix(i, "")?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn too_big() -> std::io::Error {
    std::io::Error::other("variable too large for a Level 5 MAT-file (2 GB); export fewer deployments at a time")
}

/// Write named variables to a MAT-file.
pub fn write<W: Write + Seek>(w: &mut W, vars: &[(&str, MatValue)]) -> std::io::Result<()> {
    let created = chrono::Utc::now().format("%a %b %d %H:%M:%S %Y");
    let mut text = format!("MATLAB 5.0 MAT-file, Platform: nereus, Created on: {created}").into_bytes();
    text.resize(116, b' ');
    w.write_all(&text)?;
    w.write_all(&[0u8; 8])?; // subsystem data offset: none
    w.write_all(&0x0100u16.to_le_bytes())?;
    w.write_all(b"IM")?;

    for (name, v) in vars {
        // miCOMPRESSED: tag, then the zlib stream; its length is patched in after.
        let tag_pos = w.stream_position()?;
        w.write_all(&MI_COMPRESSED.to_le_bytes())?;
        w.write_all(&0u32.to_le_bytes())?;
        let start = w.stream_position()?;
        {
            let mut out = Out { w: ZlibEncoder::new(&mut *w, Compression::new(5)) };
            out.matrix(v, name)?;
            out.w.finish()?;
        }
        let end = w.stream_position()?;
        let n = u32::try_from(end - start).map_err(|_| too_big())?;
        w.seek(SeekFrom::Start(tag_pos + 4))?;
        w.write_all(&n.to_le_bytes())?;
        w.seek(SeekFrom::Start(end))?;
    }
    w.flush()
}
