use crate::sticker::{Color, Material, Order, Sticker};
use calamine::{open_workbook_auto, Data, DataType, Reader};
use chrono::Local;
use std::collections::HashMap;
use std::error::Error;
use xlsxwriter::prelude::*;
use xlsxwriter::*;

type Coord = (usize, usize);

/// Parse orders from an Excel file
pub fn parse_orders(file_path: &str) -> Result<Vec<Order>, Box<dyn Error>> {
    let mut workbook = open_workbook_auto(&file_path)?;
    let range = workbook.worksheet_range("Sheet1")?;

    let cell1 = find_keyword_cell(&range, 6, 40, &["БГ СТИКЕР", "Френски код", "French code"])?;
    let cell2 = find_row_keyword_in_same_row(&range, cell1.0, 10, &["ПОРЪКА", "БРОЙ", "Order"])?;
    let cell3 = find_row_keyword_in_same_row(&range, cell1.0, 10, &["Описание", "Description"])?;

    let orders = extract_orders(&range, cell1.1, cell2.1, cell3.1, cell1.0)?;
    Ok(orders)
}

fn find_keyword_cell(
    range: &calamine::Range<Data>,
    max_cols: usize,
    max_rows: usize,
    keywords: &[&str],
) -> Result<Coord, Box<dyn Error>> {
    for row in 0..max_rows {
        for col in 0..max_cols {
            if let Some(val) = range.get((row, col)) {
                if let Some(s) = val.get_string() {
                    if keywords
                        .iter()
                        .any(|k| s.to_lowercase().contains(&k.to_lowercase()))
                    {
                        return Ok((row, col));
                    }
                }
            }
        }
    }
    Err("Keyword cell not found".into())
}

fn find_row_keyword_in_same_row(
    range: &calamine::Range<Data>,
    row: usize,
    max_cols: usize,
    keywords: &[&str],
) -> Result<Coord, Box<dyn Error>> {
    for col in 0..max_cols {
        if let Some(val) = range.get((row, col)) {
            if let Some(s) = val.get_string() {
                if keywords
                    .iter()
                    .any(|k| s.to_lowercase().contains(&k.to_lowercase()))
                {
                    return Ok((row, col));
                }
            }
        }
    }
    Err("Row keyword cell not found".into())
}

fn extract_orders(
    range: &calamine::Range<Data>,
    col_code: usize,
    col_amount: usize,
    col_description: usize,
    start_row: usize,
) -> Result<Vec<Order>, Box<dyn Error>> {
    let mut result = Vec::new();
    let mut started = false;

    for row in (start_row + 1)..range.height() {
        let code_opt = get_u64_from_cell(range.get((row, col_code)));

        match code_opt {
            Some(code) => {
                started = true;

                let amount = get_u64_from_cell(range.get((row, col_amount)))
                    .ok_or_else(|| format!("Invalid value in amount column at row {row}"))?;

                let description = range
                    .get((row, col_description))
                    .and_then(|cell| cell.get_string())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                result.push(Order {
                    code,
                    amount,
                    descriptions: description,
                });
            }
            None if started => break,
            None => continue,
        }
    }

    Ok(result)
}

fn get_u64_from_cell(cell: Option<&Data>) -> Option<u64> {
    match cell {
        Some(Data::Int(n)) => Some(*n as u64),
        Some(Data::Float(f)) if *f >= 0.0 && f.fract() == 0.0 => Some(*f as u64),
        Some(Data::String(s)) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

pub fn write_sizes_table(
    workbook: &mut Workbook,
    orders: &[Order],
    code_to_stickers: &HashMap<u64, Vec<Sticker>>,
) -> Result<(), XlsxError> {
    let mut sheet = workbook.add_worksheet(Some("sizes"))?;

    let headers = ["code", "description", "material", "dimensions", "amount"];
    let mut col_widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();

    let mut base_format = Format::new();
    base_format.set_border(FormatBorder::Thin);

    let mut amount_format = base_format.clone();
    amount_format.set_bg_color(FormatColor::Orange);

    for (col, header) in headers.iter().enumerate() {
        sheet.write_string(0, col as u16, header, Some(&base_format))?;
    }

    let mut row = 1;
    for order in orders {
        if let Some(stickers) = code_to_stickers.get(&order.code) {
            for sticker in stickers {
                let dims = if sticker.dimensions.len() > 1 {
                    sticker
                        .dimensions
                        .iter()
                        .enumerate()
                        .map(|(i, d)| format!("{} - {}", i + 1, d))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    sticker
                        .dimensions
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "N/A".to_string())
                };

                let values = [
                    sticker.code.to_string(),
                    sticker.description.clone(),
                    sticker.material.to_string(),
                    dims,
                    order.amount.to_string(),
                ];

                for (col, value) in values.iter().enumerate() {
                    let format = match col {
                        2 => {
                            let mut f = Format::from(sticker.material.clone());
                            f.set_border(FormatBorder::Thin);
                            f
                        }
                        3 => {
                            let mut f = Format::from(sticker.text_color.clone());
                            f.set_border(FormatBorder::Thin);
                            f
                        }
                        4 => amount_format.clone(),
                        _ => base_format.clone(),
                    };

                    sheet.write_string(row, col as u16, value, Some(&format))?;
                    col_widths[col] = col_widths[col].max(value.len());
                }

                row += 1;
            }
        }
    }

    for (col, width) in col_widths.iter().enumerate() {
        sheet.set_column(col as u16, col as u16, *width as f64 + 2.0, None)?;
    }

    Ok(())
}

pub fn write_missing_table(
    workbook: &mut Workbook,
    missing_orders: &[Order],
    code_to_stickers: &HashMap<u64, Vec<Sticker>>,
) -> Result<(), XlsxError> {
    let mut sheet = workbook.add_worksheet(Some("missing"))?;

    let headers = ["code", "description", "amount"];
    let mut col_widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();

    let mut base_format = Format::new();
    base_format.set_border(FormatBorder::Thin);

    let mut red_format = base_format.clone();
    red_format.set_font_color(FormatColor::Red);

    // Header row
    for (col, header) in headers.iter().enumerate() {
        sheet.write_string(0, col as u16, header, Some(&base_format))?;
    }

    let mut row = 1;
    for order in missing_orders
        .iter()
        .filter(|o| !code_to_stickers.contains_key(&o.code))
    {
        let values = [
            order.code.to_string(),
            order.descriptions.clone(),
            order.amount.to_string(),
        ];

        for (col, value) in values.iter().enumerate() {
            sheet.write_string(row, col as u16, value, Some(&red_format))?;
            col_widths[col] = col_widths[col].max(value.len());
        }

        row += 1;
    }

    for (col, width) in col_widths.iter().enumerate() {
        sheet.set_column(col as u16, col as u16, *width as f64 + 2.0, None)?;
    }

    Ok(())
}

pub fn write_tables<P: AsRef<std::path::Path>>(
    file_path: P,
    code_to_stickers: &HashMap<u64, Vec<Sticker>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path_str = file_path.as_ref().to_str().ok_or("Invalid file path")?;

    // Parse orders
    let orders = parse_orders(file_path_str)?;

    // Split available/missing
    let (available_orders, missing_orders): (Vec<_>, Vec<_>) = orders
        .into_iter()
        .partition(|order| code_to_stickers.contains_key(&order.code));

    let date_str = Local::now().format("%d_%m_%y").to_string();
    let new_filename = format!("orders_{}.xlsx", date_str);
    let new_path = std::path::PathBuf::from(&new_filename);

    // Create new file
    let mut workbook = Workbook::new(new_path.to_str().unwrap())?;

    write_sizes_table(&mut workbook, &available_orders, code_to_stickers)?;
    write_missing_table(&mut workbook, &missing_orders, code_to_stickers)?;

    workbook.close()?; // only close once

    Ok(())
}

impl From<Material> for Format {
    fn from(material: Material) -> Self {
        let mut format = Format::new();
        let color = match material {
            Material::PVC => format::FormatColor::Yellow,
            Material::PVCR => format::FormatColor::Orange,
            Material::PVCRSLV => format::FormatColor::Magenta,
            _ => format::FormatColor::White,
        };
        format.set_bg_color(color);
        format
    }
}

impl From<Color> for Format {
    fn from(material: Color) -> Self {
        let mut format = Format::new();
        let color = match material {
            Color::Black => format::FormatColor::Gray,
            Color::Green => format::FormatColor::Green,
            Color::Blue => format::FormatColor::Blue,
            Color::Red => format::FormatColor::Red,
        };
        format.set_bg_color(color);
        format
    }
}
