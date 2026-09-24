//! A small column store: what exports are built from, whatever the file format.

#[derive(Clone, Debug)]
pub enum Column {
    /// Numbers; NaN is missing.
    Num(Vec<f64>),
    /// Seconds since 1970, UTC; NaN is missing.
    Time(Vec<f64>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}

impl Column {
    pub fn len(&self) -> usize {
        match self {
            Column::Num(v) | Column::Time(v) => v.len(),
            Column::Str(v) => v.len(),
            Column::Bool(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Rows `from..to` as a new column.
    pub fn slice(&self, from: usize, to: usize) -> Column {
        match self {
            Column::Num(v) => Column::Num(v[from..to].to_vec()),
            Column::Time(v) => Column::Time(v[from..to].to_vec()),
            Column::Str(v) => Column::Str(v[from..to].to_vec()),
            Column::Bool(v) => Column::Bool(v[from..to].to_vec()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Table {
    pub names: Vec<String>,
    pub columns: Vec<Column>,
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: &str, col: Column) -> &mut Self {
        debug_assert!(self.columns.is_empty() || self.columns[0].len() == col.len());
        self.names.push(name.to_string());
        self.columns.push(col);
        self
    }

    pub fn n_rows(&self) -> usize {
        self.columns.first().map_or(0, Column::len)
    }

    pub fn slice(&self, from: usize, to: usize) -> Table {
        Table {
            names: self.names.clone(),
            columns: self.columns.iter().map(|c| c.slice(from, to)).collect(),
        }
    }
}

pub fn num(v: Option<f64>) -> f64 {
    v.unwrap_or(f64::NAN)
}
