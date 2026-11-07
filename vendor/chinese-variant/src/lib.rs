/*!
# Chinese Variant

An enum to represent the variants (traditional and simple) of the Chinese Language.
*/

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "enum-ordinalize")]
use enum_ordinalize::Ordinalize;

/// The different writing systems used for the Chinese language. Traditional Chinese (繁體中文) or Simple Chinese (简体中文).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "enum-ordinalize", derive(Ordinalize))]
#[repr(u8)]
pub enum ChineseVariant {
    /// 繁體中文。
    Traditional,
    /// 简体中文。
    Simple,
}

impl ChineseVariant {
    /// Is this simple?
    #[inline]
    pub const fn is_simple(self) -> bool {
        matches!(self, ChineseVariant::Simple)
    }

    /// Is this traditional?
    #[inline]
    pub const fn is_traditional(self) -> bool {
        matches!(self, ChineseVariant::Traditional)
    }
}
