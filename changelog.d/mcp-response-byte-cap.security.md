The local MCP server now cuts a query or table result at 8 MiB and marks it truncated, instead of returning every row of a result that is huge but under the row limit.
