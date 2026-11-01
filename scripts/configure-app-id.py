from pathlib import Path
import sys

source, destination, application_id = sys.argv[1:]
text = Path(source).read_text()
Path(destination).write_text(text.replace("com.bookie.linux", application_id))
