//! Data describing an operator's properties from the MathML Core operator
//! dictionary.

use std::cmp::Ordering;

use bitflags::bitflags;

#[path = "../files/mathml/data.rs"]
mod data;

/// Information about an operator.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct OperatorInfo {
    /// An optional form for this operator.
    pub form: Option<Form>,
    /// The space to the left of this operator, in em.
    pub lspace: f64,
    /// The space to the right of this operator, in em.
    pub rspace: f64,
    /// The properties this operator has.
    pub properties: Properties,
}

/// The form an operator can have.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Form {
    Infix,
    Prefix,
    Postfix,
}

bitflags! {
    /// Set of properties an operator can have.
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct Properties: u8 {
        const STRETCHY      = 1 << 0;
        const SYMMETRIC     = 1 << 1;
        const LARGEOP       = 1 << 2;
        const MOVABLELIMITS = 1 << 3;
    }
}

impl OperatorInfo {
    /// Gets the [`OperatorInfo`] struct corresponding to the given
    /// (Content, Form) pair.
    pub fn of(content: &str, form: Form, explicit_form: bool) -> &'static Self {
        let category = Self::get_operator_category(content, form);
        let category = if explicit_form || !matches!(category, data::Category::Default) {
            category
        } else {
            Self::get_implicit_operator_category(content)
        };

        data::get_category_info(category)
    }

    /// Performs Step 4 of the algorithm for determining the properties of an
    /// embellished operator, from the [MathML Core specification][spec].
    ///
    /// [spec]: https://www.w3.org/TR/mathml-core/#algorithm-for-determining-the-properties-of-an-embellished-operator
    fn get_implicit_operator_category(content: &str) -> data::Category {
        for form in [Form::Infix, Form::Postfix, Form::Prefix] {
            let category = Self::get_operator_category(content, form);
            if !matches!(category, data::Category::Default) {
                return category;
            }
        }

        data::Category::Default
    }

    /// Implementation of the algorithm to determine the category of an
    /// operator, as described in the [MathML Core specification][spec].
    ///
    /// [spec]: https://www.w3.org/TR/mathml-core/#dfn-algorithm-to-determine-the-category-of-an-operator
    fn get_operator_category(content: &str, form: Form) -> data::Category {
        use data::*;

        let content: char = match content.encode_utf16().count() {
            1 => {
                // Content must be a single codepoint in the BMP.
                let c = content.parse::<char>().unwrap();

                // Step 2.
                if ('\u{0320}'..='\u{03FF}').contains(&c) {
                    return Category::Default;
                }

                // Otherwise, don't replace content.
                c
            }
            2 => {
                // Is this a surrogate pair?
                if let Ok(c) = content.parse::<char>() {
                    // Step 2.1.
                    if matches!(c, '\u{1EEF0}' | '\u{1EEF1}') && form == Form::Postfix {
                        return Category::I;

                    // Step 2.4.
                    } else {
                        return Category::Default;
                    }
                } else {
                    // No, its two characters in BMP.
                    let mut chars = content.chars();
                    let (first, second) = (chars.next().unwrap(), chars.next().unwrap());

                    // Step 2.2.
                    if matches!(second, '\u{0338}' | '\u{20D2}') {
                        first

                    // Step 2.3.
                    } else if let Some(c) = map_two_ascii_chars(content) {
                        c

                    // Step 2.4.
                    } else {
                        return Category::Default;
                    }
                }
            }
            _ => {
                // Step 1.
                return Category::Default;
            }
        };

        // Content is now a single character in the BMP.
        debug_assert!(content <= '\u{D7FF}');

        // Step 3.
        if form == Form::Infix && matches!(content, '\u{007C}' | '\u{223C}') {
            return Category::ForceDefault;
        }
        match (content, form) {
            ('\u{2145}'..='\u{2146}' | '\u{2202}' | '\u{221A}'..='\u{221C}', Form::Prefix) => {
                return Category::L;
            }
            ('\u{002C}' | '\u{003A}' | '\u{003B}', Form::Infix) => {
                return Category::M;
            }
            _ => {}
        }

        // Step 3.1.
        let content = content as u16;
        let mut key: u16 = match content {
            0x0000..=0x03FF => content,
            0x2000..=0x2BFF => content - 0x1C00,
            _ => return Category::Default,
        };

        // Step 3.2.
        key += match form {
            Form::Infix => 0x0000,
            Form::Prefix => 0x1000,
            Form::Postfix => 0x2000,
        };

        // Step 3.3.
        debug_assert!(key <= 0x2FFF);

        // Step 3.4.
        OPERATOR_TABLE
            .binary_search_by(|&(k, data)| {
                // Note that `k` is already `% 0x4000` through the codegen.
                if key < k {
                    Ordering::Greater
                } else if key > k + (data & 0x0F) as u16 {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .map_or(Category::Default, |idx| {
                from_encoding(OPERATOR_TABLE[idx].1 >> 4)
            })
    }
}

/// Whether the given text has the fence attribute.
pub fn is_fence(content: &str) -> bool {
    map_two_ascii_chars(content)
        .or_else(|| content.parse::<char>().ok())
        .is_some_and(|c| {
            data::FENCE_TABLE
                .binary_search_by(|&(s, len)| {
                    if c < s {
                        Ordering::Greater
                    } else if c as u32 >= s as u32 + len as u32 {
                        Ordering::Less
                    } else {
                        Ordering::Equal
                    }
                })
                .is_ok()
        })
}

/// Whether the given text has the separator attribute.
pub fn is_separator(content: &str) -> bool {
    content
        .parse::<char>()
        .is_ok_and(|c| data::SEPARATOR_TABLE.contains(&c))
}

/// Whether the intrinsic stretch axis of the given Unicode character is
/// inline. If it is not inline, then it is block.
pub fn is_stretch_axis_inline(c: char) -> bool {
    if let Ok(value) = u16::try_from(c) {
        data::INLINE_AXIS_BMP_TABLE.binary_search(&value).is_ok()
    } else {
        data::INLINE_AXIS_NON_BMP_TABLE.contains(&c)
    }
}

/// Whether the given text is a single character and would be converted to its
/// italic variant if the `math-auto` property is present.
pub fn will_auto_transform(content: &str) -> bool {
    content.parse::<char>().is_ok_and(|c| {
        matches!(
            c, 'A'..='Z' | 'a'..='z' | 'ı' | 'ȷ' | 'Α'..='Ρ' | 'ϴ' | 'Σ'..='Ω'
            | '∇' | 'α'..='ω' | '∂' | 'ϵ' | 'ϑ' | 'ϰ' | 'ϕ' | 'ϱ' | 'ϖ'
        )
    })
}

fn map_two_ascii_chars(content: &str) -> Option<char> {
    data::TWO_ASCII_CHARS_TABLE
        .iter()
        .position(|&s| s == content)
        .map(|idx| char::from_u32(0x0320 + idx as u32).unwrap())
}

#[cfg(test)]
mod tests {
    use super::data::*;
    use super::*;

    #[test]
    fn test_get_operator_info() {
        assert_eq!(
            OperatorInfo::get_operator_category("", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("abc", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("+", Form::Infix),
            Category::B
        );
        assert_eq!(
            OperatorInfo::get_operator_category("+", Form::Prefix),
            Category::D
        );
        assert_eq!(
            OperatorInfo::get_operator_category("→", Form::Infix),
            Category::A
        );
        assert_eq!(
            OperatorInfo::get_operator_category("∑", Form::Prefix),
            Category::J
        );
        assert_eq!(
            OperatorInfo::get_operator_category("!", Form::Postfix),
            Category::E
        );
        assert_eq!(
            OperatorInfo::get_operator_category("|", Form::Infix),
            Category::ForceDefault
        );
        assert_eq!(
            OperatorInfo::get_operator_category("∼", Form::Infix),
            Category::ForceDefault
        );
        assert_eq!(
            OperatorInfo::get_operator_category("↼", Form::Infix),
            Category::A
        );
        assert_eq!(
            OperatorInfo::get_operator_category("|", Form::Prefix),
            Category::F
        );
        assert_eq!(
            OperatorInfo::get_operator_category("√", Form::Prefix),
            Category::L
        );
        assert_eq!(
            OperatorInfo::get_operator_category(",", Form::Infix),
            Category::M
        );
        assert_eq!(
            OperatorInfo::get_operator_category("a", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("1", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("\u{0320}", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("\u{1EEF0}", Form::Postfix),
            Category::I
        );
        assert_eq!(
            OperatorInfo::get_operator_category("\u{1EEF0}", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("\u{10437}", Form::Infix),
            Category::Default,
        );
        assert_eq!(
            OperatorInfo::get_operator_category("/\u{0338}", Form::Infix),
            Category::K
        );
        assert_eq!(
            OperatorInfo::get_operator_category("&&", Form::Infix),
            Category::B
        );
        assert_eq!(
            OperatorInfo::get_operator_category("!!", Form::Postfix),
            Category::E
        );
        assert_eq!(
            OperatorInfo::get_operator_category("||", Form::Infix),
            Category::Default
        );
        assert_eq!(
            OperatorInfo::get_operator_category("||", Form::Prefix),
            Category::D
        );
        assert_eq!(
            OperatorInfo::get_operator_category("xx", Form::Infix),
            Category::Default
        );
    }

    #[test]
    fn test_fence() {
        assert!(is_fence("\u{201D}"));
        assert!(is_fence("\u{27E7}"));
        assert!(is_fence("\u{0331}"));
        assert!(is_fence("\u{29D8}"));
        assert!(is_fence("||"));
        assert!(!is_fence("\u{232B}"));
        assert!(!is_fence("\u{2015}"));
        assert!(!is_fence("=="));
        assert!(!is_fence(""));
    }

    #[test]
    fn test_separator() {
        assert!(is_separator("\u{002C}"));
        assert!(is_separator("\u{003B}"));
        assert!(is_separator("\u{2063}"));
        assert!(!is_separator("\u{2064}"));
        assert!(!is_separator("\u{29D8}"));
        assert!(!is_separator(""));
        assert!(!is_separator("!="));
    }

    #[test]
    fn test_stretch_axis_is_inline() {
        assert!(is_stretch_axis_inline('\u{21CF}'));
        assert!(is_stretch_axis_inline('\u{1EEF0}'));
        assert!(is_stretch_axis_inline('\u{1EEF1}'));
        assert!(is_stretch_axis_inline('\u{FE38}'));
        assert!(is_stretch_axis_inline('\u{003D}'));
        assert!(!is_stretch_axis_inline('\u{295C}'));
        assert!(!is_stretch_axis_inline('\u{1D11E}'));
        assert!(!is_stretch_axis_inline('\u{1EEF2}'));
        assert!(!is_stretch_axis_inline('\u{003C}'));
    }
}
