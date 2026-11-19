A pasted or opened PostgreSQL dump with a COPY ... FROM STDIN block is no longer split at its data rows, and the block is refused with a clear message instead of any data row running as SQL.
