bookie-agentd --connect-socket now adds the token in BOOKIE_MCP_TOKEN to every request that has none, so MCP clients such as Claude Code can list and call tools through the socket.
