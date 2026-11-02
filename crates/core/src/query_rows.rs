use std::ops::{Index, IndexMut};

use crate::Value;

#[derive(Debug, Clone)]
pub struct QueryRows {
    values: Vec<Value>,
    starts: Vec<usize>,
    reversed: bool,
}

pub trait QueryRowSet {
    fn row_count(&self) -> usize;
    fn row(&self, index: usize) -> Option<&[Value]>;
}

impl QueryRows {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            starts: Vec::new(),
            reversed: false,
        }
    }

    pub fn with_capacity(rows: usize, cells: usize) -> Self {
        Self {
            values: Vec::with_capacity(cells),
            starts: Vec::with_capacity(rows),
            reversed: false,
        }
    }

    pub fn from_rows(rows: Vec<Vec<Value>>) -> Self {
        let cells = rows.iter().map(Vec::len).sum();
        let mut result = Self::with_capacity(rows.len(), cells);
        for mut row in rows {
            result.push_row(&mut row);
        }
        result
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&[Value]> {
        let row = self.physical_row(index)?;
        let start = self.starts[row];
        let end = self.starts.get(row + 1).copied().unwrap_or(self.values.len());
        Some(&self.values[start..end])
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut [Value]> {
        let row = self.physical_row(index)?;
        let start = self.starts[row];
        let end = self.starts.get(row + 1).copied().unwrap_or(self.values.len());
        Some(&mut self.values[start..end])
    }

    pub fn first(&self) -> Option<&[Value]> {
        self.get(0)
    }

    pub fn last(&self) -> Option<&[Value]> {
        self.len().checked_sub(1).and_then(|index| self.get(index))
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter { rows: self, index: 0 }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut {
            values: &mut self.values,
            starts: &self.starts,
            index: 0,
            reversed: self.reversed,
        }
    }

    pub fn push(&mut self, row: Vec<Value>) {
        let mut row = row;
        self.push_row(&mut row);
    }

    pub fn push_row(&mut self, row: &mut Vec<Value>) {
        self.normalize_order();
        self.starts.push(self.values.len());
        self.values.append(row);
    }

    pub fn remove(&mut self, index: usize) -> Vec<Value> {
        self.normalize_order();
        let start = self.starts[index];
        let end = self.starts.get(index + 1).copied().unwrap_or(self.values.len());
        let removed = self.values.drain(start..end).collect();
        let removed_cells = end - start;
        self.starts.remove(index);
        for offset in &mut self.starts[index..] {
            *offset -= removed_cells;
        }
        removed
    }

    pub fn truncate(&mut self, rows: usize) {
        if rows >= self.len() {
            return;
        }
        self.normalize_order();
        let cells = self.starts.get(rows).copied().unwrap_or(0);
        self.starts.truncate(rows);
        self.values.truncate(cells);
    }

    pub fn truncate_row(&mut self, index: usize, cells: usize) {
        let row = self.physical_row(index).unwrap_or(index);
        let start = self.starts[row];
        let end = self.starts.get(row + 1).copied().unwrap_or(self.values.len());
        if end - start <= cells {
            return;
        }
        let removed_cells = end - start - cells;
        self.values.drain(start + cells..end);
        for offset in &mut self.starts[row + 1..] {
            *offset -= removed_cells;
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.starts.clear();
        self.reversed = false;
    }

    pub fn cell_count(&self) -> usize {
        self.values.len()
    }

    pub fn contains(&self, row: &[Value]) -> bool {
        self.iter().any(|candidate| candidate == row)
    }

    pub fn into_rows(mut self) -> Vec<Vec<Value>> {
        self.normalize_order();
        let mut values = self.values.into_iter();
        let total = values.len();
        let mut rows = Vec::with_capacity(self.starts.len());
        for index in 0..self.starts.len() {
            let start = self.starts[index];
            let end = self.starts.get(index + 1).copied().unwrap_or(total);
            rows.push(values.by_ref().take(end - start).collect());
        }
        rows
    }

    pub fn reverse(&mut self) {
        if !self.is_empty() {
            self.reversed = !self.reversed;
        }
    }

    fn physical_row(&self, index: usize) -> Option<usize> {
        if index >= self.len() {
            return None;
        }
        Some(if self.reversed { self.len() - index - 1 } else { index })
    }

    fn normalize_order(&mut self) {
        if !self.reversed {
            return;
        }
        let total = self.values.len();
        let mut starts = Vec::with_capacity(self.starts.len());
        self.values.reverse();
        let mut offset = 0;
        for index in (0..self.starts.len()).rev() {
            let start = self.starts[index];
            let end = self.starts.get(index + 1).copied().unwrap_or(total);
            let row_len = end - start;
            starts.push(offset);
            self.values[offset..offset + row_len].reverse();
            offset += row_len;
        }
        self.starts = starts;
        self.reversed = false;
    }
}

impl Default for QueryRows {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<Vec<Value>>> for QueryRows {
    fn from(rows: Vec<Vec<Value>>) -> Self {
        Self::from_rows(rows)
    }
}

impl FromIterator<Vec<Value>> for QueryRows {
    fn from_iter<T: IntoIterator<Item = Vec<Value>>>(rows: T) -> Self {
        let rows = rows.into_iter();
        let mut result = Self::with_capacity(rows.size_hint().0, 0);
        for mut row in rows {
            result.push_row(&mut row);
        }
        result
    }
}

impl Index<usize> for QueryRows {
    type Output = [Value];

    fn index(&self, index: usize) -> &Self::Output {
        let row = self.physical_row(index).unwrap_or(index);
        let start = self.starts[row];
        let end = self.starts.get(row + 1).copied().unwrap_or(self.values.len());
        &self.values[start..end]
    }
}

impl IndexMut<usize> for QueryRows {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let row = self.physical_row(index).unwrap_or(index);
        let start = self.starts[row];
        let end = self.starts.get(row + 1).copied().unwrap_or(self.values.len());
        &mut self.values[start..end]
    }
}

impl<'a> IntoIterator for &'a QueryRows {
    type Item = &'a [Value];
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut QueryRows {
    type Item = &'a mut [Value];
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

pub struct Iter<'a> {
    rows: &'a QueryRows,
    index: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a [Value];

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.rows.get(self.index)?;
        self.index += 1;
        Some(row)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.rows.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Iter<'_> {}

pub struct IterMut<'a> {
    values: &'a mut [Value],
    starts: &'a [usize],
    index: usize,
    reversed: bool,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = &'a mut [Value];

    fn next(&mut self) -> Option<Self::Item> {
        let logical_index = self.index;
        let physical_index = if self.reversed {
            self.starts.len().checked_sub(logical_index + 1)?
        } else {
            logical_index
        };
        let start = *self.starts.get(physical_index)?;
        let end = self.starts.get(physical_index + 1).copied().unwrap_or_else(|| {
            if self.reversed {
                self.values.len()
            } else {
                start + self.values.len()
            }
        });
        let values = std::mem::take(&mut self.values);
        let row_len = end - start;
        let row = if self.reversed {
            let (through_row, _) = values.split_at_mut(end);
            let (remaining, row) = through_row.split_at_mut(start);
            self.values = remaining;
            row
        } else {
            let (row, remaining) = values.split_at_mut(row_len);
            self.values = remaining;
            row
        };
        self.index += 1;
        Some(row)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.starts.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for IterMut<'_> {}

impl PartialEq<Vec<Vec<Value>>> for QueryRows {
    fn eq(&self, other: &Vec<Vec<Value>>) -> bool {
        self.iter().eq(other.iter().map(Vec::as_slice))
    }
}

impl PartialEq for QueryRows {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl QueryRowSet for QueryRows {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn row(&self, index: usize) -> Option<&[Value]> {
        self.get(index)
    }
}

impl QueryRowSet for [Vec<Value>] {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn row(&self, index: usize) -> Option<&[Value]> {
        self.get(index).map(Vec::as_slice)
    }
}

impl QueryRowSet for Vec<Vec<Value>> {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn row(&self, index: usize) -> Option<&[Value]> {
        self.as_slice().row(index)
    }
}

impl<const N: usize> QueryRowSet for [Vec<Value>; N] {
    fn row_count(&self) -> usize {
        N
    }

    fn row(&self, index: usize) -> Option<&[Value]> {
        self.get(index).map(Vec::as_slice)
    }
}

#[cfg(test)]
#[path = "query_rows_tests.rs"]
mod tests;
