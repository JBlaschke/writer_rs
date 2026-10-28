//! DOCX image and picture runs (docx-output.md §4.16).
//! Owner: T41.

use crate::error::Error;
use crate::ir::{Attrs, Inlines, plain_text};

use super::validate::{XmlNode, parse_xml};
use super::{Emitter, Picture, StyleKind, xml};

const EMU_PER_TWIP: f64 = 635.0;
const EMU_PER_POINT: f64 = xml::EMU_PER_POINT as f64;
const EMU_PER_INCH: f64 = xml::EMU_PER_INCH as f64;

#[derive(Clone, Copy, Debug)]
struct Size {
    width: f64,
    height: f64,
}

impl Size {
    fn points(width: f64, height: f64) -> Self {
        let width = width * EMU_PER_POINT;
        let height = height * EMU_PER_POINT;
        if width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0
        {
            Self { width, height }
        } else {
            Self::fallback()
        }
    }

    fn fallback() -> Self {
        Self { width: 6.0 * EMU_PER_INCH, height: 4.0 * EMU_PER_INCH }
    }

    fn specified(self, attrs: &Attrs, bounds: Self) -> Self {
        let width = attrs
            .pairs
            .get("width")
            .and_then(|value| dimension(value, bounds.width));
        let height = attrs
            .pairs
            .get("height")
            .and_then(|value| dimension(value, bounds.height));
        match (width, height) {
            (Some(width), Some(height)) => Self { width, height },
            (Some(width), None) => {
                let width = width.min(bounds.width);
                Self { width, height: width * (self.height / self.width) }
            }
            (None, Some(height)) => {
                let height = height.min(bounds.height);
                Self { width: height * (self.width / self.height), height }
            }
            (None, None) => self,
        }
    }

    fn clamp(mut self, bounds: Self) -> (u64, u64, f64) {
        let mut scale = 1.0;
        if self.width > bounds.width {
            let ratio = bounds.width / self.width;
            self.width = bounds.width;
            self.height *= ratio;
            scale *= ratio;
        }
        if self.height > bounds.height {
            let ratio = bounds.height / self.height;
            self.width *= ratio;
            self.height = bounds.height;
            scale *= ratio;
        }
        (
            self.width.round().max(1.0) as u64,
            self.height.round().max(1.0) as u64,
            scale,
        )
    }
}

struct ImageData {
    raster: Option<(&'static str, Vec<u8>)>,
    svg: Option<Vec<u8>>,
    size: Size,
}

impl Emitter<'_> {
    pub(crate) fn image_run(
        &mut self, url: &str, alt: &Inlines, title: Option<&str>, attrs: &Attrs,
    ) {
        let alt = xml::single_line(&plain_text(alt));
        let Some(id) = url.strip_prefix("asset:") else {
            self.remote_image(url, &alt, title);
            return;
        };
        let bytes = match self.assets.read(id) {
            Ok(bytes) => bytes,
            Err(mut error) => {
                if let Error::MissingAsset { span, .. } = &mut error
                    && span.is_none()
                {
                    *span = self.content.span.clone();
                }
                if self.deferred_error.is_none() {
                    self.deferred_error = Some(error);
                }
                return;
            }
        };
        let mime = self
            .assets
            .get(id)
            .map(|asset| asset.mime.as_str())
            .unwrap_or("application/octet-stream");
        let Some(image) = self.asset_image(bytes, mime, attrs) else {
            self.text_run(&alt);
            return;
        };
        self.image_drawing(id, image, &alt, title, None);
    }

    fn image_bounds(&self) -> Size {
        Size {
            width: f64::from(self.available_width_twips()) * EMU_PER_TWIP,
            height: f64::from(self.layout.text_height_twips.max(1))
                * EMU_PER_TWIP,
        }
    }

    fn asset_image(
        &mut self, bytes: Vec<u8>, mime: &str, attrs: &Attrs,
    ) -> Option<ImageData> {
        if mime == "image/svg+xml"
            && !super::validate::svg_first_element(&bytes)
        {
            self.image_warning("invalid SVG image; using alt text".into());
            return None;
        }
        let intrinsic = match mime {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" => {
                let Some(size) = pixel_size(&bytes, mime) else {
                    self.image_warning(format!(
                        "invalid {mime} image; using alt text"
                    ));
                    return None;
                };
                size
            }
            "image/svg+xml" | "application/pdf" => self
                .renderer
                .image(&bytes, mime, None)
                .map(|picture| {
                    Size::points(picture.width_pt, picture.height_pt)
                })
                .unwrap_or_else(|_| Size::fallback()),
            _ => {
                self.image_warning(format!(
                    "unsupported image MIME {mime}; using alt text"
                ));
                return None;
            }
        };
        let bounds = self.image_bounds();
        let size = intrinsic.specified(attrs, bounds);
        let (width, height, _) = size.clamp(bounds);
        let size = Size { width: width as f64, height: height as f64 };
        let display_width = size.width / EMU_PER_POINT;
        let mut image = ImageData { raster: None, svg: None, size };
        match mime {
            "image/png" => image.raster = Some(("png", bytes)),
            "image/jpeg" => image.raster = Some(("jpeg", bytes)),
            "image/svg+xml" => {
                match self.renderer.image(&bytes, mime, Some(display_width)) {
                    Ok(picture) => image.raster = Some(("png", picture.png)),
                    Err(_) => self.content_warning(
                        "docx-svg-no-fallback",
                        "SVG PNG fallback failed; retaining the SVG image",
                    ),
                }
                image.svg = Some(bytes);
            }
            _ => match self.renderer.image(&bytes, mime, Some(display_width)) {
                Ok(picture) => image.raster = Some(("png", picture.png)),
                Err(_) => {
                    self.image_warning(format!(
                        "{mime} image conversion failed; using alt text"
                    ));
                    return None;
                }
            },
        }
        Some(image)
    }

    fn image_warning(&mut self, message: String) {
        self.content_warning("unsupported-construct", message);
    }

    fn remote_image(&mut self, url: &str, alt: &str, title: Option<&str>) {
        self.image_warning(format!(
            "external image {url} is a hyperlink in DOCX"
        ));
        let rel = self.package.hyperlink_rel(self.part, url);
        let tooltip = title
            .map(|title| format!(" w:tooltip=\"{}\"", xml::escape(title)))
            .unwrap_or_default();
        let props = xml::RunProps {
            style: Some(self.styles.ensure("Hyperlink", StyleKind::Character)),
            ..Default::default()
        };
        let text = if alt.is_empty() { url } else { alt };
        self.buffer().push_str(&format!(
            "<w:hyperlink r:id=\"{}\"{tooltip} w:history=\"1\"><w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:hyperlink>",
            xml::escape(&rel), props.to_xml(), xml::escape(text),
        ));
    }

    /// Shared picture seam for T42; `baseline` lowers an inline equation.
    #[allow(dead_code)]
    pub(crate) fn picture_run(
        &mut self, key: &str, picture: &Picture, alt: &str, baseline: bool,
    ) {
        let scale = normal_scale(&self.styles.to_xml());
        let size =
            Size::points(picture.width_pt * scale, picture.height_pt * scale);
        let (_, _, clamped_scale) = size.clamp(self.image_bounds());
        let position =
            (baseline && picture.baseline_pt.is_finite()).then(|| {
                (-2.0
                    * (picture.height_pt - picture.baseline_pt)
                    * scale
                    * clamped_scale)
                    .round() as i32
            });
        self.image_drawing(
            key,
            ImageData {
                raster: Some(("png", picture.png.clone())),
                svg: picture.svg.clone(),
                size,
            },
            &xml::single_line(alt),
            None,
            position,
        );
    }

    fn image_drawing(
        &mut self, key: &str, image: ImageData, alt: &str, title: Option<&str>,
        position: Option<i32>,
    ) {
        let (width, height, _) = image.size.clamp(self.image_bounds());
        let raster = image.raster.map(|(ext, bytes)| {
            self.package.image_rel(
                self.part,
                &format!("{key}:{ext}"),
                ext,
                bytes,
            )
        });
        let svg = image.svg.map(|bytes| {
            self.package.image_rel(
                self.part,
                &format!("{key}:svg"),
                "svg",
                bytes,
            )
        });
        let Some((rel, name)) = raster.as_ref().or(svg.as_ref()) else {
            return;
        };
        let svg = svg.as_ref().map(|(rel, _)| format!(
            "<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{}\"/></a:ext></a:extLst>",
            xml::escape(rel),
        )).unwrap_or_default();
        let title = title
            .map(|title| format!(" title=\"{}\"", xml::escape(title)))
            .unwrap_or_default();
        let id = self.ids.doc_pr;
        self.ids.doc_pr += 1;
        let props = xml::RunProps { position, ..Default::default() }.to_xml();
        self.buffer().push_str(&format!(concat!(
            "<w:r>{props}<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
            "<wp:extent cx=\"{width}\" cy=\"{height}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>",
            "<wp:docPr id=\"{id}\" name=\"Picture {id}\" descr=\"{alt}\"{title}/>",
            "<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>",
            "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic>",
            "<pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{name}\"/><pic:cNvPicPr><a:picLocks noChangeAspect=\"1\" noChangeArrowheads=\"1\"/></pic:cNvPicPr></pic:nvPicPr>",
            "<pic:blipFill><a:blip r:embed=\"{rel}\">{svg}</a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>",
            "<pic:spPr bwMode=\"auto\"><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm>",
            "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln w=\"9525\"><a:noFill/><a:headEnd/><a:tailEnd/></a:ln></pic:spPr>",
            "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
        ), props = props, width = width, height = height, id = id,
            title = title, svg = svg, alt = xml::escape(alt),
            name = xml::escape(name), rel = xml::escape(rel)));
    }
}

fn dimension(value: &str, available: f64) -> Option<f64> {
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let digits = &value[..split];
    let (integer, fraction) =
        digits.split_once('.').map_or((digits, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    if integer.is_empty()
        || fraction.is_some_and(|value| {
            value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return None;
    }
    let number: f64 = digits.parse().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let factor = match &value[split..] {
        "" | "px" => 9_525.0,
        "pt" => EMU_PER_POINT,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "in" => EMU_PER_INCH,
        "em" => 152_400.0,
        "%" => available / 100.0,
        _ => return None,
    };
    let value = number * factor;
    value.is_finite().then_some(value)
}

fn pixel_size(bytes: &[u8], mime: &str) -> Option<Size> {
    let kind = imagesize::image_type(bytes).ok()?;
    let matches_mime = matches!(
        (mime, kind),
        ("image/png", imagesize::ImageType::Png)
            | ("image/jpeg", imagesize::ImageType::Jpeg)
            | ("image/gif", imagesize::ImageType::Gif)
            | ("image/webp", imagesize::ImageType::Webp)
    );
    if !matches_mime {
        return None;
    }
    let size = imagesize::blob_size(bytes).ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }
    let (x, y) = match mime {
        "image/png" => png_dpi(bytes),
        "image/jpeg" => jpeg_dpi(bytes),
        _ => (96.0, 96.0),
    };
    Some(Size {
        width: size.width as f64 * EMU_PER_INCH / x,
        height: size.height as f64 * EMU_PER_INCH / y,
    })
}

fn valid_dpi(dpi: f64) -> f64 {
    if (10.0..=2_400.0).contains(&dpi) {
        dpi
    } else {
        96.0
    }
}

fn png_dpi(bytes: &[u8]) -> (f64, f64) {
    let mut offset = 8;
    while let Some(header) = bytes.get(offset..offset + 8) {
        let length =
            u32::from_be_bytes([header[0], header[1], header[2], header[3]])
                as usize;
        let Some(end) =
            offset.checked_add(12).and_then(|n| n.checked_add(length))
        else {
            break;
        };
        let Some(data) = bytes.get(offset + 8..end.saturating_sub(4)) else {
            break;
        };
        if &header[4..] == b"pHYs" && data.len() == 9 && data[8] == 1 {
            let x = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            let y = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            return (
                valid_dpi((f64::from(x) * 0.0254).round()),
                valid_dpi((f64::from(y) * 0.0254).round()),
            );
        }
        offset = end;
    }
    (96.0, 96.0)
}

fn jpeg_dpi(bytes: &[u8]) -> (f64, f64) {
    let mut offset = 2;
    while bytes.get(offset) == Some(&0xff) {
        offset += 1;
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let Some(&marker) = bytes.get(offset) else {
            break;
        };
        offset += 1;
        if matches!(marker, 0xd9 | 0xda) {
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            continue;
        }
        let Some(length) = bytes.get(offset..offset + 2) else {
            break;
        };
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        if length < 2 {
            break;
        }
        let Some(data) = bytes.get(offset + 2..offset + length) else {
            break;
        };
        if marker == 0xe0 && data.len() >= 12 && data.starts_with(b"JFIF\0") {
            let factor = match data[7] {
                1 => 1.0,
                2 => 2.54,
                _ => return (96.0, 96.0),
            };
            let x = f64::from(u16::from_be_bytes([data[8], data[9]]));
            let y = f64::from(u16::from_be_bytes([data[10], data[11]]));
            return (valid_dpi(x * factor), valid_dpi(y * factor));
        }
        offset += length;
    }
    (96.0, 96.0)
}

fn normal_scale(source: &str) -> f64 {
    let Ok(tree) = parse_xml(source) else {
        return 1.0;
    };
    let normal = tree.children.iter().find(|node| {
        node.name == "w:style"
            && node.attr("w:type") == Some("paragraph")
            && node.children.iter().any(|child| {
                child.name == "w:name"
                    && child
                        .attr("w:val")
                        .is_some_and(|name| name.eq_ignore_ascii_case("Normal"))
            })
    });
    let normal =
        normal.and_then(|node| child(node, "w:rPr")).and_then(run_size);
    let default = child(&tree, "w:docDefaults")
        .and_then(|node| child(node, "w:rPrDefault"))
        .and_then(|node| child(node, "w:rPr"))
        .and_then(run_size);
    normal.or(default).unwrap_or(22.0) / 22.0
}

fn child<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|node| node.name == name)
}

fn run_size(node: &XmlNode) -> Option<f64> {
    let size: f64 = child(node, "w:sz")?.attr("w:val")?.parse().ok()?;
    (size.is_finite() && size > 0.0).then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::registry::AssetRegistry;
    use crate::config::Config;
    use crate::docx::{
        DocxOptions, Renderer, StyleTable, ValidateOptions, validate,
    };
    use crate::error::Result;
    use crate::ir::{Document, MathDialect};

    const PNG: &[u8] =
        include_bytes!("../../tests/fixtures/docx/images/dpi144.png");
    const JPEG: &[u8] =
        include_bytes!("../../tests/fixtures/docx/images/jfif300.jpg");

    #[test]
    fn png_density_units_ranges_and_truncation() {
        assert_eq!(png_dpi(PNG), (144.0, 144.0));
        let offset =
            PNG.windows(4).position(|bytes| bytes == b"pHYs").unwrap() + 4;
        let mut bytes = PNG.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&1_u32.to_be_bytes());
        assert_eq!(png_dpi(&bytes), (96.0, 144.0));
        bytes[offset..offset + 4].copy_from_slice(&200_000_u32.to_be_bytes());
        assert_eq!(png_dpi(&bytes), (96.0, 144.0));
        bytes[offset + 8] = 0;
        assert_eq!(png_dpi(&bytes), (96.0, 96.0));
        for end in 0..PNG.len() {
            let _ = png_dpi(&PNG[..end]);
        }
    }

    #[test]
    fn jpeg_density_units_ranges_and_truncation() {
        assert_eq!(jpeg_dpi(JPEG), (300.0, 300.0));
        let offset =
            JPEG.windows(5).position(|bytes| bytes == b"JFIF\0").unwrap();
        let mut bytes = JPEG.to_vec();
        bytes[offset + 7] = 2;
        bytes[offset + 8..offset + 10].copy_from_slice(&100_u16.to_be_bytes());
        bytes[offset + 10..offset + 12].copy_from_slice(&100_u16.to_be_bytes());
        assert_eq!(jpeg_dpi(&bytes), (254.0, 254.0));
        bytes[offset + 8..offset + 10].copy_from_slice(&1_u16.to_be_bytes());
        assert_eq!(jpeg_dpi(&bytes), (96.0, 254.0));
        bytes[offset + 7] = 0;
        assert_eq!(jpeg_dpi(&bytes), (96.0, 96.0));
        for end in 0..JPEG.len() {
            let _ = jpeg_dpi(&JPEG[..end]);
        }
    }

    #[test]
    fn picture_font_scaling_prefers_normal_then_defaults() {
        let defaults = "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"33\"/></w:rPr></w:rPrDefault></w:docDefaults>";
        let normal = "<w:style w:type=\"paragraph\" w:styleId=\"Localized\"><w:name w:val=\"Normal\"/><w:rPr><w:sz w:val=\"44\"/></w:rPr></w:style>";
        let source = format!(
            "<w:styles xmlns:w=\"{}\">{defaults}{normal}</w:styles>",
            xml::NS_W
        );
        assert_eq!(normal_scale(&source), 2.0);
        assert_eq!(normal_scale(&source.replace(normal, "")), 1.5);
        assert_eq!(
            normal_scale(&source.replace(defaults, "").replace(normal, "")),
            1.0
        );
    }

    struct NoRenderer;

    impl Renderer for NoRenderer {
        fn equation(
            &mut self, _: &str, _: MathDialect, _: bool,
        ) -> Result<Picture> {
            Err(Error::Other("unused renderer".into()))
        }
        fn image(
            &mut self, _: &[u8], _: &str, _: Option<f64>,
        ) -> Result<Picture> {
            Err(Error::Other("unused renderer".into()))
        }
        fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
            Err(Error::Other("unused renderer".into()))
        }
    }

    #[test]
    fn reusable_picture_run_scales_clamps_and_positions_the_baseline() {
        let doc: Document = serde_json::from_value(serde_json::json!({
            "meta": {}, "blocks": [], "features": {}, "assets": [],
            "source": {"path": "/picture.md", "kind": "markdown", "base_dir": "/"}
        })).unwrap();
        let config = Config::default();
        let assets = AssetRegistry::new();
        let opts = DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: None,
            all_styles: false,
        };
        let mut renderer = NoRenderer;
        let mut emitter = crate::docx::prepare_emitter(
            &doc, &assets, &mut renderer, &opts, None, "en", false,
        )
        .unwrap();
        let styles =
            emitter.styles.to_xml().replace("w:val=\"22\"", "w:val=\"44\"");
        emitter.styles = StyleTable::from_reference(&styles).unwrap();
        emitter.table_width_twips = Some(200);
        let picture = Picture {
            png: PNG.to_vec(),
            svg: None,
            width_pt: 10.0,
            height_pt: 5.0,
            baseline_pt: 4.0,
        };
        emitter.start_paragraph(xml::ParaProps::default());
        emitter.picture_run("test-equation", &picture, "x  +\n y", true);
        emitter.end_paragraph();
        assert!(
            emitter.body.contains("<wp:extent cx=\"127000\" cy=\"63500\"/>")
        );
        assert!(emitter.body.contains("<w:position w:val=\"-2\"/>"));
        assert!(emitter.body.contains("descr=\"x + y\""));
        if let Some((path, _)) = emitter.package.footer() {
            let path = path.to_owned();
            let style = emitter.styles.ensure("footer", StyleKind::Paragraph);
            emitter.package.insert(
                path,
                crate::docx::reference::footer_xml(&style).into_bytes(),
                xml::CT_FOOTER,
            );
        }
        crate::docx::finish_parts(&mut emitter, None, false).unwrap();
        let bytes =
            emitter.package.finish(&doc.meta, Some("en"), None).unwrap();
        let violations =
            validate(&bytes, &ValidateOptions { builtin_styles: true });
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn nonfinite_rendition_dimensions_have_a_finite_positive_fallback() {
        let bounds = Size { width: 100_000.0, height: 100_000.0 };
        for (width, height) in
            [(f64::NAN, 1.0), (1.0, f64::INFINITY), (-1.0, 1.0), (1e308, 1e308)]
        {
            let (width, height, _) = Size::points(width, height).clamp(bounds);
            assert_eq!((width, height), (100_000, 66_667));
        }
    }
}
