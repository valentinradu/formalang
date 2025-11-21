# FormaLens

Navigation and understanding toolkit for FormaLang projects.

## Components

### Grammar (`grammar/`)

Tree-sitter grammar for FormaLang syntax highlighting and structural queries.

```bash
cd formalens/grammar
npm install
npm run generate
npm run test
```

Use with ast-grep:
```bash
ast-grep --lang formalang -p 'pub struct $NAME { $$$ }'
```

### Semantic Search (`semantic/`)

LanceDB-powered semantic search using local embeddings.

```bash
# Build
cd formalens/semantic
cargo build --release

# Index the codebase
cargo run -- index --root /path/to/project

# Search
cargo run -- search "how does error handling work"

# Clear index
cargo run -- clear
```

## Integration

### VSCode

Add to `.vscode/settings.json`:
```json
{
  "files.associations": {
    "*.fv": "formalang"
  }
}
```

### Claude Code

The semantic search CLI can be used to find relevant context:
```bash
formalens search "trait implementation patterns"
```

## File Patterns

By default, indexes:
- `*.md` - Markdown documentation (chunked by headers)
- `*.fv` - FormaLang source (chunked by definitions)
- `*.rs` - Rust source (chunked by items)

## Index Location

Index stored at `~/.formalens/index.lancedb`
