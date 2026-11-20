import sys, json, re, statistics, subprocess, os, difflib
from PIL import Image, ImageDraw, ImageFont
src, outdir, ocr, dump = sys.argv[1:5]
rgb = Image.open(src).convert('RGB'); im = rgb.convert('L')
X0,X1,Y0,Y1,COLS,ROWS = 205,627,58,564,5,6          # рамка сетки (в продукте — автопоиск)
cw,ch = (X1-X0)/COLS, (Y1-Y0)/ROWS
def edge(pts):
    h=0
    for x,y,dx,dy in pts:
        c=max(im.getpixel((x+dx*o,y+dy*o)) for o in (-1,0,1))
        s=(im.getpixel((x-dx*4,y-dy*4))+im.getpixel((x+dx*4,y+dy*4)))/2
        h+= c-s>10
    return h/len(pts)
occ={}
for r in range(ROWS):
    for c in range(COLS):
        px=[im.getpixel((round(X0+c*cw+x),round(Y0+r*ch+y))) for x in range(10,75,4) for y in range(10,75,4)]
        occ[(r,c)]=statistics.pstdev(px)>8
parent={k:k for k in occ if occ[k]}
def find(k):
    while parent[k]!=k: k=parent[k]
    return k
for (r,c) in list(parent):
    if (r,c+1) in parent and edge([(round(X0+(c+1)*cw),y,1,0) for y in range(round(Y0+r*ch+12),round(Y0+(r+1)*ch-12))])<0.5:
        parent[find((r,c+1))]=find((r,c))
    if (r+1,c) in parent and edge([(x,round(Y0+(r+1)*ch),0,1) for x in range(round(X0+c*cw+12),round(X0+(c+1)*cw-12))])<0.5:
        parent[find((r+1,c))]=find((r,c))
groups={}
for k in parent: groups.setdefault(find(k),[]).append(k)
items=json.load(open(f'{dump}/items.json'))['data']['items']
en=json.load(open(f'{dump}/items_en.json'))['data']; ru=json.load(open(f'{dump}/items_ru.json'))['data']
norm=lambda s: re.sub(r'[^0-9a-z]','',s.lower())
short={}
for iid in items:
    sn=en.get(iid+' ShortName','')
    if sn: short.setdefault(norm(sn),[]).append(iid)
def price(it):
    f=0 if 'noFlea' in it.get('types',[]) else (it.get('lastLowPrice') or 0)
    return max([f]+[s.get('priceRUB') or 0 for s in it.get('sellToTrader',[])])
draw=ImageDraw.Draw(rgb); total=0; rows=[]
for i,(root,cells) in enumerate(sorted(groups.items())):
    rs=[r for r,c in cells]; cs=[c for r,c in cells]
    w,h=max(cs)-min(cs)+1, max(rs)-min(rs)+1
    bx0,by0=X0+min(cs)*cw, Y0+min(rs)*ch; bx1,by1=X0+(max(cs)+1)*cw, Y0+(max(rs)+1)*ch
    # OCR только правый верхний угол квадрата
    crop=rgb.crop((int(bx0+2),int(by0+1),int(bx1-1),int(by0+22))).resize((int((bx1-bx0)*3),63))
    p=f'{outdir}/c{i}.png'; crop.save(p)
    txt=' '.join(l.split('\t')[0] for l in subprocess.run([ocr,p],capture_output=True,text=True).stdout.splitlines()).strip()
    n=norm(txt)
    cands=short.get(n) or [x for k,v in short.items() if len(n)>=4 and k.startswith(n) for x in v] \
          or [x for k in difflib.get_close_matches(n,list(short),1,0.75) for x in short[k]]
    sized=[x for x in cands if (items[x]['width'],items[x]['height']) in ((w,h),(h,w))]
    pick=sized or cands
    color=(80,220,120) if pick else (255,90,90)
    draw.rectangle([bx0+2,by0+2,bx1-2,by1-2],outline=color,width=3)
    if pick:
        it=max((items[x] for x in pick),key=price); v=price(it); total+=v
        rows.append(f"{w}×{h}  «{txt}» → {en.get(it['id']+' ShortName')}: {ru.get(it['id']+' Name','')[:34]}  {v:,} ₽  (кандидатов {len(cands)} → по размеру {len(sized) or len(cands)})".replace(',',' '))
        draw.rectangle([bx0+4,by1-20,bx0+4+8*len(f'{v//1000}k'),by1-5],fill=(0,0,0))
        draw.text((bx0+6,by1-19),f'{v//1000}k',fill=color)
    else:
        rows.append(f"{w}×{h}  «{txt}» → не опознан")
print('\n'.join(rows)); print(f'ИТОГО ≈ {total:,} ₽'.replace(',',' '))
rgb.save(f'{outdir}/../backpack_boxes.png')
