---
name: feishu-doc
description: Create and manage Feishu documents and access Feishu MCP tools
---

# Feishu MCP Tools

The following tools are available through the standard Feishu MCP service. They are automatically discovered and don't require separate configuration.

## Document Tools
- `create-doc` — Create a new cloud document (requires: title)
- `fetch-doc` — Read document content (requires: docID)
- `search-doc` — Search documents by keyword (requires: query)
- `update-doc` — Update document content (requires: docID + content)
- `list-docs` — List documents in a knowledge space
- `get-comments` — View document comments (requires: docID)
- `add-comments` — Add a comment (requires: docID + content)

## User Tools
- `search-user` — Search for colleagues (requires: query)
- `get-user` — Get user details (requires: userID)

## File Tools
- `fetch-file` — Retrieve file content (requires: fileToken)

## Instructions
You have access to Feishu cloud services via MCP tools. **Always use Feishu cloud documents instead of local files or markdown when writing documentation, meeting notes, proposals, or any content that needs to be shared with others.**

## Best Practices
1. **Cloud-first**: Always use Feishu cloud documents for any content needing sharing
2. **Clear titles**: Use descriptive titles so documents can be found via search
3. **Share URLs**: After creating a document, share the returned URL with the user
4. **Search first**: Search for existing documents before creating new ones
5. **Review**: Use `fetch-doc` + `get-comments` to review documents and feedback