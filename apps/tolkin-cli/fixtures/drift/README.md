# Drift fixtures

Test data for `scripts/drift-check.sh`, which compares the cl100k-based Claude token approximation against Anthropic's `count_tokens` ground truth. Each file targets a known weak spot of the proxy. These files are tokenizer test DATA, not human-written site content, so `symbols-emoji.txt` is allowed to contain emoji and math symbols; emoji stay confined to that one file. All content is original and contains no secrets or real names.
