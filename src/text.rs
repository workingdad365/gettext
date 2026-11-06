//! 추출한 텍스트 조합, 특수 공백 변환, Tab 키 선택 순환

/// 줄바꿈을 CRLF 로 통일하고 끝의 줄바꿈 제거
pub fn normalize_part(part: &str) -> String {
    let lf = part.replace("\r\n", "\n").replace('\r', "\n");
    lf.trim_end_matches('\n').replace('\n', "\r\n")
}

/// 요소의 이름, 값, 설명 등 여러 조각을 줄바꿈으로 이은 결과
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Joined {
    pub text: String,
    /// 두 번째 조각부터 각 조각이 시작하는 위치 (UTF-16 단위, EDIT 컨트롤 선택 위치와 같은 단위)
    pub indexes: Vec<i32>,
}

impl Joined {
    pub fn push(&mut self, part: &str) {
        let part = normalize_part(part);
        if part.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push_str("\r\n");
            self.indexes.push(utf16_len(&self.text));
        }
        self.text.push_str(&part);
    }

    pub fn from_parts<S: AsRef<str>>(parts: &[S], ascii_spaces: bool) -> Joined {
        let mut joined = Joined::default();
        for p in parts {
            if ascii_spaces {
                joined.push(&unicode_spaces_to_ascii(p.as_ref()));
            } else {
                joined.push(p.as_ref());
            }
        }
        joined
    }
}

pub fn utf16_len(s: &str) -> i32 {
    s.encode_utf16().count() as i32
}

/// 특수 공백은 일반 공백으로 바꾸고 보이지 않는 서식 문자는 제거
pub fn unicode_spaces_to_ascii(s: &str) -> String {
    s.chars()
        .filter(|c| !is_invisible(*c))
        .map(|c| if is_unicode_space(c) { ' ' } else { c })
        .collect()
}

fn is_unicode_space(c: char) -> bool {
    matches!(c, '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
}

fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206B}'
            | '\u{206E}'
            | '\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

/// Tab 키를 누를 때 다음으로 선택할 범위.
/// 전체 선택 -> 첫 조각 -> 다음 조각 -> ... -> 전체 선택 순서로 순환
pub fn next_tab_selection(sel: (i32, i32), len: i32, indexes: &[i32]) -> (i32, i32) {
    const NEWLINE: i32 = 2;
    let (start, end) = sel;
    if start == 0 && end == len {
        let first_end = indexes.first().map_or(len, |i| i - NEWLINE);
        return (0, first_end);
    }
    for (i, &index) in indexes.iter().enumerate() {
        let from = if i == 0 { 0 } else { indexes[i - 1] };
        let to = index - NEWLINE;
        if from == start && to == end {
            let next_end = indexes.get(i + 1).map_or(len, |n| n - NEWLINE);
            return (index, next_end);
        }
    }
    (0, len)
}

/// 크기 계산용 텍스트. DrawText 와 EDIT 컨트롤의 줄바꿈 위치 차이를 줄이기 위해 구두점 주변에 공백 추가
pub fn measure_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '!' | '|' | '?' | '-' | '}' => {
                out.push(c);
                out.push(' ');
            }
            '{' | '[' | '(' | '+' | '%' | '$' | '\\' => {
                out.push(' ');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_part("a\nb\r\nc\rd\n\n"), "a\r\nb\r\nc\r\nd");
        assert_eq!(normalize_part("\n\n"), "");
        assert_eq!(normalize_part("x"), "x");
    }

    #[test]
    fn join_parts_and_indexes() {
        let j = Joined::from_parts(&["이름", "", "값\n", "설명"], false);
        assert_eq!(j.text, "이름\r\n값\r\n설명");
        // "이름\r\n" = 4, "이름\r\n값\r\n" = 7 (UTF-16)
        assert_eq!(j.indexes, vec![4, 7]);

        let single = Joined::from_parts(&["only"], false);
        assert_eq!(single.indexes, Vec::<i32>::new());
        assert_eq!(Joined::from_parts::<&str>(&[], false), Joined::default());
    }

    #[test]
    fn indexes_count_utf16_units() {
        // 이모지는 UTF-16 두 단위
        let j = Joined::from_parts(&["😀", "b"], false);
        assert_eq!(j.indexes, vec![4]);
    }

    #[test]
    fn unicode_spaces() {
        assert_eq!(unicode_spaces_to_ascii("a\u{00A0}b\u{3000}c\u{2009}d"), "a b c d");
        assert_eq!(unicode_spaces_to_ascii("a\u{200B}b\u{FEFF}c\u{00AD}d\u{202E}"), "abcd");
        assert_eq!(unicode_spaces_to_ascii("한글 abc"), "한글 abc");
        // 변환 후 조각 위치가 맞아야 함
        let j = Joined::from_parts(&["a\u{200B}b", "c"], true);
        assert_eq!(j.text, "ab\r\nc");
        assert_eq!(j.indexes, vec![4]);
    }

    #[test]
    fn tab_cycle() {
        // "ab\r\ncd\r\nef": len 10, indexes [4, 8]
        let idx = [4, 8];
        let len = 10;
        assert_eq!(next_tab_selection((0, 10), len, &idx), (0, 2));
        assert_eq!(next_tab_selection((0, 2), len, &idx), (4, 6));
        assert_eq!(next_tab_selection((4, 6), len, &idx), (8, 10));
        assert_eq!(next_tab_selection((8, 10), len, &idx), (0, 10));
        // 임의 선택이면 전체 선택으로
        assert_eq!(next_tab_selection((1, 3), len, &idx), (0, 10));
        // 조각이 하나면 전체 선택 유지
        assert_eq!(next_tab_selection((0, 5), 5, &[]), (0, 5));
    }

    #[test]
    fn measure_spacing() {
        assert_eq!(measure_text("a-b(c)"), "a- b (c)");
        assert_eq!(measure_text("plain"), "plain");
    }
}
