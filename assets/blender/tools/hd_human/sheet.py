"""sheet.py OUT.png cols cell_h img... : alpha onto light grey, crop to content, label with file stem."""
import sys, os
from PIL import Image, ImageDraw
out, cols, ch = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
ims = []
for p in sys.argv[4:]:
    im = Image.open(p).convert("RGBA")
    bb = im.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox() or (0, 0, *im.size)
    pad = 12
    im = im.crop((max(0, bb[0]-pad), max(0, bb[1]-pad), min(im.width, bb[2]+pad), min(im.height, bb[3]+pad)))
    bg = Image.new("RGBA", im.size, (226, 227, 230, 255)); bg.alpha_composite(im)
    s = ch / bg.height
    ims.append((os.path.splitext(os.path.basename(p))[0], bg.convert("RGB").resize((max(1, int(bg.width*s)), ch), Image.LANCZOS)))
rows = [ims[i:i+cols] for i in range(0, len(ims), cols)]
W = max(sum(i.width for _, i in r) for r in rows)
sheet = Image.new("RGB", (W, len(rows)*(ch+26)), (226, 227, 230))
d = ImageDraw.Draw(sheet)
y = 0
for r in rows:
    x = 0
    for n, i in r:
        sheet.paste(i, (x, y+26)); d.text((x+6, y+6), n, fill=(20, 20, 20)); x += i.width
    y += ch + 26
sheet.save(out); print(out, sheet.size)
