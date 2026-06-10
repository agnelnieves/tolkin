// File text extraction. Everything runs in-memory in the browser: a file is
// read with the File/Blob APIs and its text returned. Nothing is uploaded.
//
// Every non-trivial parser (PDF, DOCX, XLSX, and the file-type sniffer) is
// dynamically imported on first use so first paint stays light. Text formats
// take the cheap path: we read the raw bytes as text and pass them through
// untouched, because we count exactly what the model would receive (no markdown
// or YAML stripping).

export type ParsedKind = "text" | "pdf" | "docx" | "xlsx";

export type ParseResult = {
  kind: ParsedKind;
  text: string;
};

// Extensions we treat as plain text. Anything here (and anything that sniffs as
// text) is read verbatim. This is deliberately broad: code, configs, and markup
// are all "what the model receives".
const TEXT_EXTENSIONS = new Set([
  "md",
  "mdx",
  "markdown",
  "txt",
  "text",
  "json",
  "jsonc",
  "json5",
  "yaml",
  "yml",
  "toml",
  "ini",
  "cfg",
  "conf",
  "csv",
  "tsv",
  "xml",
  "svg",
  "html",
  "htm",
  "css",
  "scss",
  "less",
  "ts",
  "tsx",
  "mts",
  "cts",
  "js",
  "jsx",
  "mjs",
  "cjs",
  "py",
  "rb",
  "rs",
  "go",
  "java",
  "kt",
  "kts",
  "c",
  "h",
  "cc",
  "cpp",
  "hpp",
  "cxx",
  "cs",
  "swift",
  "m",
  "mm",
  "php",
  "sh",
  "bash",
  "zsh",
  "fish",
  "ps1",
  "bat",
  "sql",
  "graphql",
  "gql",
  "proto",
  "lua",
  "pl",
  "r",
  "jl",
  "dart",
  "scala",
  "clj",
  "ex",
  "exs",
  "erl",
  "vue",
  "svelte",
  "astro",
  "env",
  "gitignore",
  "dockerfile",
  "makefile",
  "log",
]);

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot < 0 || dot === name.length - 1) {
    // No extension: fall back to the bare (lowercased) filename so files like
    // "Dockerfile", "Makefile", ".gitignore" still route to the text path.
    return name.toLowerCase().replace(/^\./, "");
  }
  return name.slice(dot + 1).toLowerCase();
}

export async function parseFile(file: File): Promise<ParseResult> {
  const ext = extensionOf(file.name);

  if (ext === "pdf") {
    return { kind: "pdf", text: await parsePdf(file) };
  }
  if (ext === "docx") {
    return { kind: "docx", text: await parseDocx(file) };
  }
  if (ext === "xlsx" || ext === "xlsm" || ext === "xlsb" || ext === "xls") {
    return { kind: "xlsx", text: await parseXlsx(file) };
  }
  if (TEXT_EXTENSIONS.has(ext)) {
    return { kind: "text", text: await file.text() };
  }

  // Unknown extension: sniff the bytes. If it sniffs as a binary office/PDF
  // type, route accordingly; otherwise read as text. The sniffer is lazy.
  const sniffed = await sniffKind(file);
  switch (sniffed) {
    case "pdf":
      return { kind: "pdf", text: await parsePdf(file) };
    case "docx":
      return { kind: "docx", text: await parseDocx(file) };
    case "xlsx":
      return { kind: "xlsx", text: await parseXlsx(file) };
    default:
      return { kind: "text", text: await file.text() };
  }
}

// Detect the file type by content for extensionless or ambiguous files. Only
// the four kinds we can extract are distinguished; everything else is "text".
async function sniffKind(file: File): Promise<ParsedKind> {
  try {
    const { fileTypeFromBuffer } = await import("file-type");
    const head = await file.slice(0, 4100).arrayBuffer();
    const result = await fileTypeFromBuffer(new Uint8Array(head));
    if (!result) return "text";
    if (result.mime === "application/pdf") return "pdf";
    if (result.mime === "application/vnd.openxmlformats-officedocument.wordprocessingml.document") {
      return "docx";
    }
    if (result.mime === "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet") {
      return "xlsx";
    }
    return "text";
  } catch {
    return "text";
  }
}

async function parsePdf(file: File): Promise<string> {
  const pdfjs = await import("pdfjs-dist");
  // Point the worker at the bundled asset URL so nothing is fetched from a CDN
  // (privacy rule). Turbopack rewrites this new URL() form to a local asset.
  pdfjs.GlobalWorkerOptions.workerSrc = new URL(
    "pdfjs-dist/build/pdf.worker.min.mjs",
    import.meta.url,
  ).toString();

  const data = await file.arrayBuffer();
  // Keep the loading task: destroy() (which tears down the worker) lives on the
  // task, while the resolved document exposes getPage/cleanup.
  const loadingTask = pdfjs.getDocument({ data });
  const doc = await loadingTask.promise;
  const pages: string[] = [];
  try {
    for (let pageNum = 1; pageNum <= doc.numPages; pageNum++) {
      const page = await doc.getPage(pageNum);
      const content = await page.getTextContent();
      const line = content.items
        .map((item) => ("str" in item ? item.str : ""))
        .join(" ")
        .trim();
      pages.push(line);
      page.cleanup();
    }
  } finally {
    await loadingTask.destroy();
  }
  return pages.join("\n");
}

async function parseDocx(file: File): Promise<string> {
  const mammoth = (await import("mammoth")).default;
  const arrayBuffer = await file.arrayBuffer();
  const result = await mammoth.extractRawText({ arrayBuffer });
  return result.value;
}

async function parseXlsx(file: File): Promise<string> {
  const XLSX = await import("xlsx");
  const data = await file.arrayBuffer();
  const workbook = XLSX.read(data, { type: "array" });
  const parts: string[] = [];
  for (const name of workbook.SheetNames) {
    const sheet = workbook.Sheets[name];
    if (!sheet) continue;
    // sheet_to_csv gives a compact, model-relevant text view of cell contents.
    parts.push(XLSX.utils.sheet_to_csv(sheet));
  }
  return parts.join("\n");
}
