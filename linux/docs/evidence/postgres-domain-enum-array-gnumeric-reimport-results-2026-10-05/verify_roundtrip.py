from pathlib import Path
from zipfile import ZipFile
from xml.etree import ElementTree as ET

BASE = Path(__file__).parent
EXPECTED = '{"NULL","","東京","a,b","a\\"b","<tag>&","=1+1",NULL}'
NS = {
    "x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "table": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
    "office": "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
}


def xlsx_cell(path):
    with ZipFile(path) as archive:
        sheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        shared = (
            ET.fromstring(archive.read("xl/sharedStrings.xml"))
            if "xl/sharedStrings.xml" in archive.namelist()
            else None
        )
        assert sheet.find(".//x:f", NS) is None, ET.tostring(sheet)
        cell = sheet.find('.//x:c[@r="A2"]', NS)
        assert cell is not None and cell.get("t") in ("s", "inlineStr"), ET.tostring(sheet)
        if cell.get("t") == "inlineStr":
            return cell.findtext("x:is/x:t", namespaces=NS)
        index = int(cell.findtext("x:v", namespaces=NS))
        return "".join(shared.find(f"x:si[{index + 1}]", NS).itertext())


def ods_cell(path):
    with ZipFile(path) as archive:
        root = ET.fromstring(archive.read("content.xml"))
        assert root.find('.//*[@table:formula]', NS) is None, ET.tostring(root)
        rows = root.findall(".//table:table-row", NS)
        cell = rows[1].find("table:table-cell", NS)
        assert cell is not None and cell.get(f'{{{NS["office"]}}}value-type') == "string"
        return "".join(cell.itertext())


for name, reader in (
    ("source.xlsx", xlsx_cell),
    ("gnumeric-roundtrip.ods", ods_cell),
    ("gnumeric-roundtrip.xlsx", xlsx_cell),
):
    actual = reader(BASE / name)
    assert actual == EXPECTED, f"{name}: {actual!r} != {EXPECTED!r}"

assert '"=1+1"' in EXPECTED
assert ',NULL}' in EXPECTED
print("Gnumeric preserved domain-over-enum array text, formula-shaped label, empty/literal NULL distinctions and no-formula string cell across XLSX/ODS/XLSX")
