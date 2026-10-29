#!/usr/bin/env python3
"""Generate modern SVG badges for BookiE supported database engines and core features."""

import os

BADGES_DIR = os.path.join(os.path.dirname(__file__), "..", "docs", "brand", "badges")
os.makedirs(BADGES_DIR, exist_ok=True)

# Brand palette:
# Ledger Green: #1F4F48, Deep Ink: #163D38, Night: #0E2421, Parchment: #F3EAD6, Linen: #FBF7EC, Sage: #9EAE9A, Brass: #C9A24B

ENGINES = [
    ("postgres", "PostgreSQL", "STABLE", "#336791", "#1F4F48", "#9EAE9A"),
    ("mysql", "MySQL / MariaDB", "STABLE", "#00758F", "#1F4F48", "#9EAE9A"),
    ("sqlite", "SQLite", "STABLE", "#003B57", "#1F4F48", "#9EAE9A"),
    ("mssql", "SQL Server", "STABLE", "#CC292B", "#1F4F48", "#9EAE9A"),
    ("clickhouse", "ClickHouse", "STABLE", "#FFCC01", "#1F4F48", "#9EAE9A"),
    ("duckdb", "DuckDB", "OPTIONAL", "#FFF000", "#163D38", "#C9A24B"),
    ("redis", "Redis", "EXPERIMENTAL", "#D82C20", "#163D38", "#C9A24B"),
    ("mongodb", "MongoDB", "EXPERIMENTAL", "#47A248", "#163D38", "#C9A24B"),
]

def make_engine_badge(slug, name, status, icon_accent, status_bg, status_text_color):
    # Compute width based on name length
    name_len = len(name)
    status_len = len(status)
    name_px = name_len * 7.5
    status_px = status_len * 6.5 + 16
    width = int(42 + name_px + status_px + 12)
    status_x = width - int(status_px) - 8

    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="32" viewBox="0 0 {width} 32" role="img" aria-label="{name} {status}">
  <rect width="{width}" height="32" rx="16" fill="#0E2421" stroke="#1F4F48" stroke-width="1"/>
  <g transform="translate(10, 7)">
    <rect x="0" y="0" width="18" height="18" rx="4" fill="{icon_accent}" opacity="0.15"/>
    <path d="M4 4c0-1.2 2.2-2 5-2s5 .8 5 2v10c0 1.2-2.2 2-5 2s-5-.8-5-2V4z" fill="#1F4F48"/>
    <ellipse cx="9" cy="4" rx="5" ry="1.8" fill="{icon_accent}"/>
    <path d="M4 7c0 .9 2.2 1.6 5 1.6s5-.7 5-1.6" fill="none" stroke="#9EAE9A" stroke-width="0.8"/>
    <path d="M4 10c0 .9 2.2 1.6 5 1.6s5-.7 5-1.6" fill="none" stroke="#9EAE9A" stroke-width="0.8"/>
  </g>
  <text x="36" y="20" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif" font-size="12" font-weight="600" fill="#F3EAD6">{name}</text>
  <rect x="{status_x}" y="7" width="{int(status_px)}" height="18" rx="9" fill="{status_bg}"/>
  <text x="{status_x + int(status_px)/2}" y="19.5" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif" font-size="9" font-weight="700" fill="{status_text_color}" text-anchor="middle">{status}</text>
</svg>
'''
    return svg

for slug, name, status, accent, sbg, stxt in ENGINES:
    filepath = os.path.join(BADGES_DIR, f"{slug}.svg")
    with open(filepath, "w", encoding="utf-8") as f:
        f.write(make_engine_badge(slug, name, status, accent, sbg, stxt))

# Feature badges
FEATURES = [
    ("badge-gtk4", "Linux Native", "GTK4 & Libadwaita", "#1F4F48", "#C9A24B"),
    ("badge-policy", "PolicyGuard", "Zero-Account Gate", "#1F4F48", "#9EAE9A"),
    ("badge-keyring", "Secret Service", "Keyring Protected", "#1F4F48", "#9EAE9A"),
    ("badge-mcp", "AI Assistant", "MCP Ready", "#1F4F48", "#C9A24B"),
]

for slug, title, sub, bg, accent in FEATURES:
    t_len = len(title) * 7.5
    s_len = len(sub) * 6.5
    w = int(max(t_len, s_len) + 48)
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="32" viewBox="0 0 {w} 32" role="img" aria-label="{title}: {sub}">
  <rect width="{w}" height="32" rx="16" fill="#0E2421" stroke="#1F4F48" stroke-width="1"/>
  <circle cx="16" cy="16" r="6" fill="{accent}"/>
  <text x="32" y="15" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif" font-size="10" font-weight="700" fill="{accent}" letter-spacing="0.5">{title.upper()}</text>
  <text x="32" y="25" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif" font-size="9.5" font-weight="500" fill="#F3EAD6">{sub}</text>
</svg>
'''
    filepath = os.path.join(BADGES_DIR, f"{slug}.svg")
    with open(filepath, "w", encoding="utf-8") as f:
        f.write(svg)

print(f"Generated badges in {BADGES_DIR}")
