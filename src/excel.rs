use anyhow::{Context, Result};
use calamine::{open_workbook_auto, DataType, Reader};
use simple_excel_writer as excel;
use std::path::Path;

pub struct Workbook {
    sheets: Vec<Sheet>,
    current_sheet_index: usize,
    file_path: String,
    is_modified: bool,
}

pub struct Sheet {
    pub name: String,
    pub data: Vec<Vec<Cell>>,
    pub max_rows: usize,
    pub max_cols: usize,
}

#[derive(Clone)]
pub struct Cell {
    pub value: String,
    pub is_formula: bool,
    pub cell_type: CellType,
    pub original_type: Option<DataTypeInfo>,
}

#[derive(Clone, PartialEq)]
pub enum CellType {
    Text,
    Number,
    Date,
    Boolean,
    Empty,
}

#[derive(Clone, PartialEq)]
pub enum DataTypeInfo {
    Empty,
    String,
    Float(f64),
    Int(i64),
    Bool(bool),
    DateTime(f64),
    Duration(f64),
    DateTimeIso(String),
    DurationIso(String),
    Error,
}

impl Cell {
    pub fn new(value: String, is_formula: bool) -> Self {
        // Determine cell type based on content
        let cell_type = if value.is_empty() {
            CellType::Empty
        } else if is_formula {
            CellType::Text // Formulas are treated as text initially
        } else if value.parse::<f64>().is_ok() {
            CellType::Number
        } else if value.contains('/') && value.split('/').count() == 3 {
            // Simple date detection (e.g., 2021/01/01)
            CellType::Date
        } else if value.contains('-') && value.split('-').count() == 3 {
            // Simple date detection (e.g., 2021-01-01)
            CellType::Date
        } else if value == "true" || value == "false" {
            CellType::Boolean
        } else {
            CellType::Text
        };

        Self::new_with_type(value, is_formula, cell_type, None)
    }

    pub fn new_with_type(
        value: String,
        is_formula: bool,
        cell_type: CellType,
        original_type: Option<DataTypeInfo>,
    ) -> Self {
        Self {
            value,
            is_formula,
            cell_type,
            original_type,
        }
    }

    pub fn empty() -> Self {
        Self {
            value: String::new(),
            is_formula: false,
            cell_type: CellType::Empty,
            original_type: Some(DataTypeInfo::Empty),
        }
    }
}

pub fn open_workbook<P: AsRef<Path>>(path: P) -> Result<Workbook> {
    let path_str = path.as_ref().to_string_lossy().to_string();

    // Open workbook directly from path
    let mut workbook = open_workbook_auto(&path).context("Unable to parse Excel file")?;

    let sheet_names = workbook.sheet_names().to_vec();
    let mut sheets = Vec::new();

    for name in &sheet_names {
        let range = workbook
            .worksheet_range(name)
            .context(format!("Unable to read worksheet: {}", name))?;
        let sheet = create_sheet_from_range(name, range?);
        sheets.push(sheet);
    }

    if sheets.is_empty() {
        anyhow::bail!("No worksheets found in file");
    }

    Ok(Workbook {
        sheets,
        current_sheet_index: 0,
        file_path: path_str,
        is_modified: false,
    })
}

fn create_sheet_from_range(name: &str, range: calamine::Range<DataType>) -> Sheet {
    let height = range.height();
    let width = range.width();

    let mut data = vec![vec![Cell::empty(); width + 1]; height + 1];

    for (row_idx, row) in range.rows().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            // Extract value, cell_type, and original_type from the DataType
            let (value, cell_type, original_type) = match cell {
                DataType::Empty => (String::new(), CellType::Empty, Some(DataTypeInfo::Empty)),
                DataType::String(s) => (s.to_string(), CellType::Text, Some(DataTypeInfo::String)),
                DataType::Float(f) => (
                    f.to_string(),
                    CellType::Number,
                    Some(DataTypeInfo::Float(*f)),
                ),
                DataType::Int(i) => (i.to_string(), CellType::Number, Some(DataTypeInfo::Int(*i))),
                DataType::Bool(b) => (
                    b.to_string(),
                    CellType::Boolean,
                    Some(DataTypeInfo::Bool(*b)),
                ),
                DataType::Error(e) => (
                    format!("Error: {:?}", e),
                    CellType::Text,
                    Some(DataTypeInfo::Error),
                ),
                DataType::DateTime(dt) => (
                    format!("{}", dt),
                    CellType::Date,
                    Some(DataTypeInfo::DateTime(*dt)),
                ),
                DataType::Duration(d) => (
                    format!("{}", d),
                    CellType::Text,
                    Some(DataTypeInfo::Duration(*d)),
                ),
                DataType::DateTimeIso(s) => (
                    s.to_string(),
                    CellType::Date,
                    Some(DataTypeInfo::DateTimeIso(s.to_string())),
                ),
                DataType::DurationIso(s) => (
                    s.to_string(),
                    CellType::Text,
                    Some(DataTypeInfo::DurationIso(s.to_string())),
                ),
            };

            // Can't directly determine if it's a formula; calamine limitation
            // Roughly determine by checking if value starts with '='
            let is_formula = value.starts_with('=');

            data[row_idx + 1][col_idx + 1] =
                Cell::new_with_type(value, is_formula, cell_type, original_type);
        }
    }

    Sheet {
        name: name.to_string(),
        data,
        max_rows: height,
        max_cols: width,
    }
}

impl Workbook {
    pub fn get_current_sheet(&self) -> &Sheet {
        &self.sheets[self.current_sheet_index]
    }

    pub fn set_cell_value(&mut self, row: usize, col: usize, value: String) -> Result<()> {
        if row >= self.sheets[self.current_sheet_index].data.len()
            || col >= self.sheets[self.current_sheet_index].data[0].len()
        {
            anyhow::bail!("Cell coordinates out of range");
        }

        let is_formula = value.starts_with('=');

        // Use Cell::new which handles type detection
        self.sheets[self.current_sheet_index].data[row][col] = Cell::new(value, is_formula);
        self.is_modified = true;

        Ok(())
    }

    pub fn get_sheet_names(&self) -> Vec<String> {
        self.sheets.iter().map(|s| s.name.clone()).collect()
    }

    pub fn get_current_sheet_name(&self) -> String {
        self.sheets[self.current_sheet_index].name.clone()
    }

    pub fn switch_sheet(&mut self, index: usize) -> Result<()> {
        if index >= self.sheets.len() {
            anyhow::bail!("Sheet index out of range");
        }

        self.current_sheet_index = index;
        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        self.is_modified
    }

    pub fn get_file_path(&self) -> &str {
        &self.file_path
    }

    pub fn save(&mut self) -> Result<()> {
        let mut excel_wb = excel::Workbook::create(&self.file_path);

        for sheet in &self.sheets {
            let mut excel_sheet = excel_wb.create_sheet(&sheet.name);

            // Set column widths
            for _ in 0..sheet.max_cols {
                excel_sheet.add_column(excel::Column { width: 15.0 });
            }

            // Write all cell data
            excel_wb.write_sheet(&mut excel_sheet, |sheet_writer| {
                // Write data by row
                for row in 1..sheet.data.len() {
                    if row <= sheet.max_rows {
                        // Create a row of data
                        let mut excel_row = excel::Row::new();

                        for col in 1..sheet.data[0].len() {
                            if col <= sheet.max_cols {
                                if !sheet.data[row][col].value.is_empty() {
                                    excel_row.add_cell(sheet.data[row][col].value.clone());
                                } else {
                                    excel_row.add_cell(String::new());
                                }
                            }
                        }

                        // Only add row if not empty
                        sheet_writer.append_row(excel_row)?;
                    }
                }

                Ok(())
            })?;
        }

        // Close workbook, save to file
        excel_wb.close()?;

        // Reset modification flag
        self.is_modified = false;

        Ok(())
    }
}
