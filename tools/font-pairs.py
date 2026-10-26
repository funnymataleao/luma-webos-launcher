"""Offline static OFL font generation for Luma, retaining Latin and Cyrillic."""
import json
import sys
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools import subset

source, output = map(Path, sys.argv[1:3])
output.mkdir(parents=True, exist_ok=True)
unicodes = (list(range(0x20,0x250))+list(range(0x400,0x530))+
    list(range(0x1C80,0x1C90))+list(range(0x2000,0x2070))+
    list(range(0x20A0,0x20D0))+[0x2116,0x2122,0x2190,0x2192,0x2212,0x2713,0x2714,0x25CF,0x20BF])
faces = [
 ("archivo","display",800,{"wdth":112.5,"wght":800}),
 ("fraunces","display",700,{"opsz":72,"SOFT":50,"WONK":0,"wght":700}),
 ("unbounded","display",700,{"wght":700}),
 ("inter","display",700,{"opsz":32,"wght":700}),
 ("inter","display",800,{"opsz":32,"wght":800}),
 ("inter","text",400,{"opsz":14,"wght":400}),
 ("inter","text",500,{"opsz":14,"wght":500}),
 ("inter","text",600,{"opsz":14,"wght":600}),
 ("manrope","text",400,{"wght":400}),
 ("manrope","text",500,{"wght":500}),
 ("manrope","text",600,{"wght":600}),
]
metrics={}; report=[]
for family, role, weight, axes in faces:
 font=TTFont(source/f"{family}.ttf")
 available={x.axisTag for x in font["fvar"].axes}
 if missing:=set(axes)-available:
  raise ValueError(f"{family}: missing {missing}")
 static=instantiateVariableFont(font,axes,inplace=False)
 opts=subset.Options()
 opts.layout_features=["*"]
 opts.notdef_glyph=True
 opts.notdef_outline=True
 s=subset.Subsetter(options=opts)
 s.populate(unicodes=unicodes)
 s.subset(static)
 filename=f"font-pair-{family}-{role}-{weight}.ttf"
 static.save(output/filename)
 upem=static["head"].unitsPerEm
 cmap=static.getBestCmap()
 adv=static["hmtx"].metrics
 metrics[f"{family}-{role}-{weight}"]={chr(code):adv[name][0]/upem for code,name in cmap.items()}
 if role == "text" and any(ord(c) not in cmap for c in "КинопоискДенис"):
  raise ValueError(f"{filename}: Cyrillic missing in required text fallback")
 report.append({"name":filename,"bytes":(output/filename).stat().st_size,"axes":axes})
(output/"font-pair-metrics.json").write_text(json.dumps(metrics,ensure_ascii=False,separators=(",",":")),encoding="utf-8")
for family in ("archivo","fraunces","unbounded","inter","manrope"):
 (output/f"font-pair-{family}-OFL.txt").write_bytes((source/f"{family}-OFL.txt").read_bytes())
total=sum(f["bytes"] for f in report)
print(json.dumps({"total_bytes":total,"faces":report},indent=2))
if total>3000000: raise ValueError(f"Typography budget exceeded: {total}")
