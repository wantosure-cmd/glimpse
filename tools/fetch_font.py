"""Fetch OFL Chinese font and produce a small app-specific subset for fast startup."""
import hashlib, json, pathlib, urllib.request
root=pathlib.Path(__file__).resolve().parent.parent
cache=root/'vendor/downloads'
assets=root/'assets'
assets.mkdir(exist_ok=True)
base='https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/'
font_url=base+'NotoSansSC%5Bwght%5D.ttf'
path=cache/'NotoSansSC-variable.ttf'
if not path.exists():
    with urllib.request.urlopen(font_url,timeout=90) as src:path.write_bytes(src.read())
if not (assets/'NotoSansSC-OFL.txt').exists():
    with urllib.request.urlopen(base+'OFL.txt',timeout=60) as src:(assets/'NotoSansSC-OFL.txt').write_bytes(src.read())
from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
font=TTFont(path)
font=instantiateVariableFont(font,{'wght':400},inplace=True)
full=cache/'NotoSansSC-Regular.ttf'
font.save(full)
# All product spec and Rust UI text, plus ASCII and common punctuation.
text=''.join(p.read_text(encoding='utf8') for p in list((root/'src').glob('**/*.rs'))+[root/'产品需求与技术方案.md'])
options=subset.Options()
options.name_IDs=['*']
subsetter=subset.Subsetter(options=options)
subsetter.populate(text=text+''.join(chr(n) for n in range(32,127)))
subsetter.subset(font)
font.save(assets/'ui-chinese.ttf')
(assets/'ui-charset.txt').write_text(''.join(chr(n) for n in sorted(font.getBestCmap())),encoding='utf8')
record={'source':font_url,'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'output_sha256':hashlib.sha256((assets/'ui-chinese.ttf').read_bytes()).hexdigest(),'full_sha256':hashlib.sha256(full.read_bytes()).hexdigest(),'weight':400,'license':'SIL Open Font License 1.1','note':'Subset and complete filename fallback both instantiated at regular weight 400.'}
(assets/'font-manifest.json').write_text(json.dumps(record,indent=2),encoding='utf8')
print('font bytes:',(assets/'ui-chinese.ttf').stat().st_size)
