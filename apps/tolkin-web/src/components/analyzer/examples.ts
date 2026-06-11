// Inline sample payloads for the analyzer's first-run example chips. Each is a
// realistic but compact piece of input so a non-technical visitor can land on
// the page, click a chip, and immediately see the panels populate without
// pasting anything of their own. The strings live in code (not fetched) so
// nothing leaves the browser.

export type AnalyzerExample = {
  id: "prompt" | "mcp" | "claudemd";
  label: string;
  blurb: string;
  text: string;
};

const SAMPLE_PROMPT = `You are an SDR copilot for a B2B SaaS company.

Goals:
- Qualify inbound leads from website form submissions
- Draft a personalized first-touch reply within 5 minutes
- Hand off booked meetings to AEs with a one-paragraph briefing

Voice guide:
- Plain English, no exclamation marks, no emoji
- Lead with the prospect's problem, not the product
- Keep replies under 120 words

Tools available:
- crm.search(query, limit)
- crm.create_note(deal_id, body)
- email.draft(to, subject, body)
- calendar.find_slot(invitees, duration_minutes)

If you cannot find a deal in CRM, ask the user before creating a new one.
Never send an email; always draft and hand it back for review.`;

const SAMPLE_MCP = `{
  "mcpServers": {
    "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] },
    "linear": { "url": "https://mcp.linear.app/sse", "type": "sse" },
    "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "~"] },
    "notion": { "command": "npx", "args": ["@notionhq/notion-mcp-server"] }
  }
}`;

const SAMPLE_CLAUDEMD = `# Project Instructions for Claude

This is a Next.js 16 + TypeScript app deployed to Vercel.

## Conventions

- Use bun for all package management (never npm or yarn)
- Strict TypeScript, no any, prefer unknown then narrow
- Server Components by default, "use client" only when required
- Tailwind v4 with the design tokens in globals.css
- Tests with vitest and playwright; snapshot tests are forbidden

## Repository layout

- apps/web is the marketing site
- apps/api is the public API (Hono on Cloudflare Workers)
- packages/ui contains the shared component library
- packages/db wraps Drizzle ORM around a Postgres database

## Working agreements

- Open a draft PR after the first commit and update it as you go
- Run "bun test" and "bun lint" before pushing
- If a change touches packages/ui, regenerate Storybook with "bun storybook:build"
- Never commit a .env file; secrets go in Vercel and Cloudflare`;

export const ANALYZER_EXAMPLES: AnalyzerExample[] = [
  {
    id: "prompt",
    label: "Sample prompt",
    blurb: "A short SDR copilot system prompt with tool definitions.",
    text: SAMPLE_PROMPT,
  },
  {
    id: "claudemd",
    label: "Sample CLAUDE.md",
    blurb: "A project instructions file like the ones agents read on boot.",
    text: SAMPLE_CLAUDEMD,
  },
  {
    id: "mcp",
    label: "Sample MCP config",
    blurb: "An MCP servers block with a few well-known servers wired in.",
    text: SAMPLE_MCP,
  },
];
