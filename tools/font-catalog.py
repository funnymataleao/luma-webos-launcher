"""Official OFL Google Fonts: download source once, subset Latin+Cyrillic+UI for TV."""
import json, urllib.request, pathlib, sys
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools import subset
ROOT=pathlib.Path(__file__).resolve().parent.parent
SOURCE=ROOT/'assets/fonts'; OUT=ROOT/'.build/assets'
SOURCE.mkdir(exist_ok=True,parents=True); OUT.mkdir(exist_ok=True,parents=True)
HEADING=['archivo','inter','unbounded','montserrat','manrope','onest','rubik','exo2','raleway','oswald','russoone','jost','playfairdisplay','prata','fraunces','spacegrotesk','syne','bebasneue','nunito']
TEXT=['inter','manrope','onest','golostext','rubik','montserrat','ibmplexsans','notosans','sourcesans3','roboto','ptsans','opensans','nunitosans','jost','nunito']
FAMILIES=list(dict.fromkeys(HEADING+TEXT))
UC=list(range(0x20,0x250))+list(range(0x400,0x530))+list(range(0x1C80,0x1C90))+list(range(0x2000,0x2070))+list(range(0x20A0,0x20D0))+[0x2116,0x2122,0x2190,0x2192,0x2212,0x2713,0x2714,0x25CF,0x20BF]
REQUIRED='КинопоискДенисПривет'
NAMES={'archivo':'Archivo Expanded','inter':'Inter','unbounded':'Unbounded','montserrat':'Montserrat','manrope':'Manrope','onest':'Onest','rubik':'Rubik','exo2':'Exo 2','raleway':'Raleway','oswald':'Oswald','russoone':'Russo One','jost':'Jost','playfairdisplay':'Playfair Display','prata':'Prata','fraunces':'Fraunces','spacegrotesk':'Space Grotesk','syne':'Syne','bebasneue':'Bebas Neue','golostext':'Golos Text','ibmplexsans':'IBM Plex Sans','notosans':'Noto Sans','sourcesans3':'Source Sans 3','roboto':'Roboto','ptsans':'PT Sans','opensans':'Open Sans','nunitosans':'Nunito Sans','nunito':'Nunito'}
def req(url):
 return urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'Luma-OpenFont-LicenseAudit'}),timeout=45).read()
if len(sys.argv)>1 and sys.argv[1]=='fetch':
 for fam in FAMILIES:
  if (SOURCE/f'{fam}-catalog.ttf').is_file() and (SOURCE/f'{fam}-catalog-OFL.txt').is_file(): continue
  try:
   listing=json.loads(req(f'https://api.github.com/repos/google/fonts/contents/ofl/{fam}'))
   names=[x for x in listing if x['name'].lower().endswith('.ttf') and 'italic' not in x['name'].lower()]
   names.sort(key=lambda x:(0 if '[' in x['name'] else 1, 0 if 'regular' in x['name'].lower() else 1,len(x['name'])))
   lic=next((x for x in listing if x['name'] in ('OFL.txt','LICENSE.txt')),None)
   if not names or not lic: print('SKIP',fam,'source/license absent',flush=True); continue
   for file,dest in ((names[0],SOURCE/f'{fam}-catalog.ttf'),(lic,SOURCE/f'{fam}-catalog-OFL.txt')):
    dest.write_bytes(req(file['download_url']))
   print('FETCH',fam,names[0]['name'],flush=True)
  except Exception as e: print('FAIL',fam,str(e),flush=True)
 sys.exit(0)
metrics={};catalog=[];total=0
for fam in FAMILIES:
 path=SOURCE/f'{fam}-catalog.ttf';license=SOURCE/f'{fam}-catalog-OFL.txt'
 if not path.is_file() or not license.is_file(): print('SKIP',fam,'not downloaded');continue
 lic=license.read_text(errors='replace')
 if 'OPEN FONT LICENSE' not in lic.upper() and 'APACHE LICENSE' not in lic.upper():print('SKIP',fam,'license not approved');continue
 font=TTFont(path)
 axes={a.axisTag:(a.minValue,a.defaultValue,a.maxValue) for a in font['fvar'].axes} if font.get('fvar') else {}
 cmap=font.getBestCmap()
 cyr=all(ord(ch) in cmap for ch in REQUIRED)
 units=font['head'].unitsPerEm
 cap=getattr(font.get('OS/2'),'sCapHeight',0) or int(units*.72)
 xh=getattr(font.get('OS/2'),'sxHeight',0) or int(units*.53)
 roles=[]
 if fam in HEADING: roles.append(('heading',[800,900] if fam=='nunito' else [800 if fam in ('archivo','inter','unbounded') else 700]))
 if fam in TEXT and cyr: roles.append(('text',[400,500,600]))
 if not roles: continue
 for role,weights in roles:
  for weight in weights:
   ft=TTFont(path)
   if axes:
    vals={k:(min(max(weight,lo),hi) if k=='wght' else min(max(32 if role=='heading' else 14,lo),hi) if k=='opsz' else de) for k,(lo,de,hi) in axes.items()}
    ft=instantiateVariableFont(ft,vals,inplace=False)
   opts=subset.Options();opts.layout_features=['*'];opts.notdef_glyph=True;opts.notdef_outline=True
   su=subset.Subsetter(options=opts);su.populate(unicodes=UC);su.subset(ft)
   filename=f'font-catalog-{fam}-{role}-{weight}.ttf'
   ft.save(OUT/filename)
   m=ft.getBestCmap();adv=ft['hmtx'].metrics;up=ft['head'].unitsPerEm
   metrics[f'{fam}-{role}-{weight}']={chr(k):adv[v][0]/up for k,v in m.items()}
   total+=(OUT/filename).stat().st_size
 (OUT/f'font-catalog-{fam}-OFL.txt').write_bytes(license.read_bytes())
 catalog.append({'family':fam,'name':NAMES[fam],'roles':[r for r,_ in roles],'cyrillic':cyr,'license':'OFL','cap_ratio':cap/units,'x_ratio':xh/units,'source_bytes':path.stat().st_size,
   'subset_bytes':sum(p.stat().st_size for p in OUT.glob(f'font-catalog-{fam}-*.ttf'))})
 print('BUILT',fam,'cyr=',cyr,'total=',total,flush=True)
(OUT/'font-catalog-metrics.json').write_text(json.dumps(metrics,ensure_ascii=False,separators=(',',':')))
(OUT/'font-catalog.json').write_text(json.dumps(catalog,indent=2))
print('FONT TOTAL',total, 'HEADINGS',sum('heading' in x['roles'] for x in catalog),'TEXT',sum('text' in x['roles'] for x in catalog))
if total>8*1024*1024:raise SystemExit('8 MiB font budget exceeded')
if sum('heading' in x['roles'] for x in catalog)<14 or sum('text' in x['roles'] for x in catalog)<12:raise SystemExit('Insufficient licensed families')
