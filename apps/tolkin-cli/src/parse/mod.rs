use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;

/// Extract the text a model would actually receive from a file on disk.
/// Text formats are already UTF-8 and pass through unchanged. Binary document
/// formats (PDF, DOCX, XLSX) are decoded to their visible text. Sniffing is by
/// extension first, then by magic bytes for extensionless or ambiguous files.
pub fn extract(path: &Path) -> Result<String> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => return extract_pdf(path),
            "docx" => return extract_docx(path),
            "xlsx" | "xlsm" | "xlsb" | "ods" => return extract_xlsx(path),
            // Known text and source formats are UTF-8 already.
            _ if is_text_extension(ext) => return read_text(path),
            // Unknown extension: fall through to magic-byte sniffing.
            _ => {}
        }
    }
    extract_by_magic(path)
}

/// Magic-byte fallback for extensionless or unknown-extension files. DOCX and
/// XLSX share the ZIP signature, so `infer` is asked for the specific OOXML
/// mime type. Anything not recognized as a supported binary is treated as text.
fn extract_by_magic(path: &Path) -> Result<String> {
    let kind = infer::get_from_path(path)
        .with_context(|| format!("failed to read {} for type sniffing", path.display()))?;
    match kind.map(|k| k.mime_type()) {
        Some("application/pdf") => extract_pdf(path),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document") => {
            extract_docx(path)
        }
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet") => {
            extract_xlsx(path)
        }
        _ => read_text(path),
    }
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("failed to extract text from {}", path.display()))
}

fn extract_pdf(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path)
        .with_context(|| format!("failed to extract text from {}", path.display()))
}

/// A .docx is a ZIP; the body lives in word/document.xml. Pull the runs from
/// <w:t> elements and break a line on every <w:p> paragraph boundary.
fn extract_docx(path: &Path) -> Result<String> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to extract text from {}", path.display()))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .with_context(|| format!("no word/document.xml in {}", path.display()))?
        .read_to_string(&mut xml)
        .with_context(|| format!("failed to extract text from {}", path.display()))?;

    let mut reader = XmlReader::from_str(&xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader
            .read_event()
            .with_context(|| format!("failed to extract text from {}", path.display()))?
        {
            Event::Start(e) if local_name(e.name().as_ref()) == b"t" => in_text = true,
            Event::End(e) if local_name(e.name().as_ref()) == b"t" => in_text = false,
            Event::End(e) if local_name(e.name().as_ref()) == b"p" => out.push('\n'),
            Event::Text(e) if in_text => {
                let raw = e
                    .decode()
                    .with_context(|| format!("failed to extract text from {}", path.display()))?;
                match quick_xml::escape::unescape(&raw) {
                    Ok(text) => out.push_str(&text),
                    Err(_) => out.push_str(&raw),
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Concatenate non-empty cell values tab separated per row, with a blank line
/// between sheets, so the result mirrors what a model would read off the grid.
fn extract_xlsx(path: &Path) -> Result<String> {
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("failed to extract text from {}", path.display()))?;
    let mut out = String::new();
    for (i, name) in workbook.sheet_names().into_iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let range = workbook
            .worksheet_range(&name)
            .with_context(|| format!("failed to read sheet {name} in {}", path.display()))?;
        for row in range.rows() {
            let mut first = true;
            for cell in row {
                if matches!(cell, Data::Empty) {
                    continue;
                }
                if !first {
                    out.push('\t');
                }
                out.push_str(&cell.to_string());
                first = false;
            }
            out.push('\n');
        }
    }
    Ok(out)
}

/// Strip an XML namespace prefix (for example w:t becomes t) so element checks
/// do not depend on the document's chosen prefix.
fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().position(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

/// Extensions known to be UTF-8 text or source code. These bypass sniffing.
fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "markdown"
            | "txt"
            | "text"
            | "json"
            | "jsonc"
            | "json5"
            | "yaml"
            | "yml"
            | "toml"
            | "csv"
            | "tsv"
            | "xml"
            | "html"
            | "htm"
            | "env"
            | "ini"
            | "cfg"
            | "conf"
            | "log"
            | "rs"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "py"
            | "rb"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "cxx"
            | "hpp"
            | "cs"
            | "php"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "sql"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "vue"
            | "svelte"
            | "lua"
            | "pl"
            | "r"
            | "dart"
            | "scala"
            | "clj"
            | "ex"
            | "exs"
            | "erl"
            | "hs"
            | "ml"
            | "fs"
            | "graphql"
            | "gql"
            | "proto"
            | "dockerfile"
            | "makefile"
            | "gradle"
            | "properties"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn text_extension_passthrough() {
        let dir = std::env::temp_dir();
        let path = dir.join("tolkin_parse_test.md");
        fs::write(&path, "# hello\n\nsome **markdown** text\n").unwrap();
        let got = extract(&path).unwrap();
        assert!(got.contains("some **markdown** text"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn unknown_extension_text_falls_back_to_read() {
        let dir = std::env::temp_dir();
        let path = dir.join("tolkin_parse_test.weirdext");
        fs::write(&path, "plain content here").unwrap();
        let got = extract(&path).unwrap();
        assert_eq!(got, "plain content here");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn docx_extracts_paragraph_text() {
        // A .docx is a ZIP with word/document.xml. Synthesize a minimal one.
        let dir = std::env::temp_dir();
        let path = dir.join("tolkin_parse_test.docx");
        let body = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>First paragraph.</w:t></w:r></w:p>
<w:p><w:r><w:t>Second </w:t><w:t>paragraph.</w:t></w:r></w:p>
</w:body></w:document>"#;
        let file = fs::File::create(&path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(body.as_bytes()).unwrap();
        zw.finish().unwrap();

        let got = extract(&path).unwrap();
        assert!(got.contains("First paragraph."), "got: {got:?}");
        assert!(got.contains("Second paragraph."), "got: {got:?}");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn xlsx_extracts_cells_tab_separated() {
        // Build a minimal valid xlsx (two sheets, inline strings) so calamine
        // can read it without an external fixture or writer crate.
        let dir = std::env::temp_dir();
        let path = dir.join("tolkin_parse_test.xlsx");
        write_minimal_xlsx(&path);

        let got = extract(&path).unwrap();
        assert!(got.contains("Name\tTokens"), "got: {got:?}");
        assert!(got.contains("hello\t42"), "got: {got:?}");
        // A blank line separates the two sheets, then the second sheet's cell.
        assert!(got.contains("\n\nsecond sheet cell"), "got: {got:?}");
        fs::remove_file(&path).ok();
    }

    fn write_minimal_xlsx(path: &std::path::Path) {
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut zw = zip::ZipWriter::new(fs::File::create(path).unwrap());

        let members: [(&str, &str); 5] = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
<sheet name="Sheet1" sheetId="1" r:id="rId1"/>
<sheet name="Sheet2" sheetId="2" r:id="rId2"/>
</sheets>
</workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
</Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Tokens</t></is></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>hello</t></is></c><c r="B2"><v>42</v></c></row>
</sheetData>
</worksheet>"#,
            ),
        ];
        for (name, body) in members {
            zw.start_file(name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        let sheet2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>second sheet cell</t></is></c></row>
</sheetData>
</worksheet>"#;
        zw.start_file("xl/worksheets/sheet2.xml", opts).unwrap();
        zw.write_all(sheet2.as_bytes()).unwrap();
        zw.finish().unwrap();
    }
}
