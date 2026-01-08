use crate::schema::MpsVec2;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TileRange {
    pub start: MpsVec2,
    pub end: MpsVec2,
}

impl TileRange {
    pub fn area(self) -> usize {
        (self.end.x - self.start.x + 1) as usize * (self.end.y - self.start.y + 1) as usize
    }
}

impl IntoIterator for TileRange {
    type Item = MpsVec2;
    type IntoIter = TileRangeIterator;

    fn into_iter(self) -> Self::IntoIter {
        assert!(self.end.x >= self.start.x && self.end.y >= self.start.y);
        TileRangeIterator {
            range: self,
            current: Some(self.start),
        }
    }
}

#[derive(Clone)]
pub struct TileRangeIterator {
    range: TileRange,
    current: Option<MpsVec2>,
}

impl Iterator for TileRangeIterator {
    type Item = MpsVec2;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.current?;
        let mut next = result;
        if result.x < self.range.end.x {
            next.x += 1;
            self.current = Some(next);
        } else if result.y < self.range.end.y {
            next.x = self.range.start.x;
            next.y += 1;
            self.current = Some(next);
        } else {
            self.current = None;
        }
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn nth(&mut self, mut n: usize) -> Option<Self::Item> {
        let mut current = self.current?;

        let remaining_on_first_line = (self.range.end.x - current.x + 1) as usize;
        if n >= remaining_on_first_line {
            if current.y < self.range.end.y {
                current.y += 1;
            } else {
                self.current = None;
                return None;
            }
            current.x = self.range.start.x;
            n -= remaining_on_first_line;
        }

        if n > 0 {
            let per_line = (self.range.end.x - self.range.start.x + 1) as usize;
            let remaining_full_lines = (self.range.end.y - current.y) as usize;
            let skipped_lines = n / per_line;
            n %= per_line;
            if skipped_lines > remaining_full_lines {
                self.current = None;
                return None;
            }
            current.y += skipped_lines as i32;
            current.x += n as i32;
        }

        self.current = Some(current);
        self.next()
    }
}

impl ExactSizeIterator for TileRangeIterator {
    fn len(&self) -> usize {
        let Some(current) = self.current else {
            return 0;
        };
        let remaining_in_line = (self.range.end.x - current.x + 1) as usize;
        let line_size = (self.range.end.x - self.range.start.x + 1) as usize;
        let remaining_lines = (self.range.end.y - current.y) as usize;
        remaining_in_line + line_size * remaining_lines
    }
}

#[cfg(test)]
mod tests {
    use super::TileRange;
    use crate::schema::MpsVec2;
    use itertools::Itertools;

    macro_rules! make_iter {
        (($x1:literal, $y1:literal) - ($x2:literal, $y2:literal)) => {
            TileRange {
                start: MpsVec2::new($x1, $y1),
                end: MpsVec2::new($x2, $y2),
            }
            .into_iter()
        };
    }

    #[test]
    fn test_iter() {
        macro_rules! test_iter {
            ($start:tt - $end:tt => [$(($x:literal, $y:literal)),+ $(,)?]) => {
                assert_eq!(
                    make_iter!($start - $end).collect_vec(),
                    vec![$(MpsVec2::new($x, $y)),+]
                )
            };
        }

        test_iter!((0, 0) - (0, 0) => [(0, 0)]);
        test_iter!((0, 0) - (2, 0) => [(0, 0), (1, 0), (2, 0)]);
        test_iter!((0, 0) - (0, 2) => [(0, 0), (0, 1), (0, 2)]);
        test_iter!((0, 0) - (2, 2) => [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)]);
    }

    #[test]
    fn test_iter_nth() {
        assert_eq!(make_iter!((0, 0) - (4, 4)).nth(0), Some(MpsVec2::new(0, 0)));
        assert_eq!(make_iter!((0, 0) - (4, 4)).nth(2), Some(MpsVec2::new(2, 0)));
        assert_eq!(
            make_iter!((0, 0) - (4, 4)).nth(12),
            Some(MpsVec2::new(2, 2))
        );
        assert_eq!(
            make_iter!((0, 0) - (4, 4)).nth(22),
            Some(MpsVec2::new(2, 4))
        );
        assert_eq!(
            make_iter!((0, 0) - (4, 4)).nth(24),
            Some(MpsVec2::new(4, 4))
        );
        assert_eq!(make_iter!((0, 0) - (4, 4)).nth(25), None);

        let mut iter = make_iter!((0, 0) - (4, 4));
        iter.next();
        iter.next();
        assert_eq!(iter.nth(5), Some(MpsVec2::new(2, 1)));

        let mut iter = make_iter!((0, 0) - (4, 4));
        #[allow(clippy::iter_nth_zero)]
        iter.nth(0);
        assert_eq!(iter.next(), Some(MpsVec2::new(1, 0)));

        let mut iter = make_iter!((0, 0) - (4, 4));
        iter.nth(12);
        assert_eq!(iter.next(), Some(MpsVec2::new(3, 2)));

        let mut iter = make_iter!((0, 0) - (4, 4));
        iter.nth(24);
        assert_eq!(iter.next(), None);
    }
}
