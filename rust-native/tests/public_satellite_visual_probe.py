"""Inspect actually decoded pixel diversity/dimensions of no-login satellite images."""
import datetime as dt, io, json, pathlib, re, urllib.parse, urllib.request
from PIL import Image, ImageStat
Image.MAX_IMAGE_PIXELS=120_000_000
out=pathlib.Path("source-image-samples");out.mkdir(exist_ok=True)
now=dt.datetime.now(dt.timezone.utc)
results=[]
def fetch(url,limit=30*1024*1024):
    req=urllib.request.Request(url,headers={"User-Agent":"CurrentEarthWallpaper/1.2 source review",
              "Accept":"image/jpeg,image/png,image/*;q=0.9,*/*;q=0.5"})
    with urllib.request.urlopen(req,timeout=20) as r:
        data=r.read(limit+1)
        return data,r.headers.get("Content-Type","")
def test(name,url):
    try:
        data,ct=fetch(url)
        if len(data)>30*1024*1024: raise ValueError("Over 30 MiB; unreasonable wallaper load")
        image=Image.open(io.BytesIO(data))
        image.load()
        rgb=image.convert("RGB")
        thumb=rgb.copy()
        thumb.thumbnail((512,512))
        stat=ImageStat.Stat(thumb)
        samples=thumb.resize((64,64))
        unique=len(samples.getcolors(maxcolors=4096) or [])
        d={"name":name,"ok":True,"type":image.format,"width":image.width,
          "height":image.height,"byte_size":len(data),"mean_rgb":[round(x,1) for x in stat.mean],
          "stddev_rgb":[round(x,1) for x in stat.stddev],"unique_colors_64x64":unique,
          "usable_color_variation":max(stat.stddev)>12 and unique>100,"url":url}
        thumb.save(out/(name+".jpg"),quality=82)
    except Exception as e:
        d={"name":name,"ok":False,"error":str(e)[:330],"url":url}
    results.append(d)
    print(json.dumps(d,ensure_ascii=False),flush=True)
base="https://view.eumetsat.int/geoserver/wms"
q={"SERVICE":"WMS","VERSION":"1.1.1","REQUEST":"GetMap","STYLES":"",
 "FORMAT":"image/jpeg","SRS":"EPSG:4326","BBOX":"-29.5,-75,120.5,75","WIDTH":"800","HEIGHT":"800"}
for layer,name in [("msg_iodc:rgb_naturalenhncd","meteosat9_enhanced"),
                   ("msg_iodc:rgb_natural","meteosat9_natural")]:
    q["LAYERS"]=layer;test(name,base+"?"+urllib.parse.urlencode(q))
q.update({"LAYERS":"mtg_fd:rgb_geocolour","BBOX":"-77,-77,77,77"})
test("existing_meteosat12",base+"?"+urllib.parse.urlencode(q))

# Fresh GK2A filenames, avoid guessed fixed latest path; try candidate UTC time slots.
host="https://nmsc.kma.go.kr/IMG/GK2A/AMI/PRIMARY/L1B/COMPLETE/FD"
for lag in [30,50,70,100,130,190,300]:
    t=now-dt.timedelta(minutes=lag)
    t=t.replace(minute=(t.minute//10)*10,second=0,microsecond=0)
    url=f"{host}/{t:%Y%m}/{t:%d}/{t:%H}/gk2a_ami_le1b_rgb-true_fd010ge_{t:%Y%m%d%H%M}.srv.png"
    try:
        data,ct=fetch(url,limit=30*1024*1024)
        image=Image.open(io.BytesIO(data));image.verify()
        test("gk2a_truecolor",url);break
    except Exception as e:
        if lag==300:print("GK2A recent image attempts failed",str(e)[:180])
# Elektro-L4 current thumbnail, with possible higher-resolution link discovery.
page="https://electro.ntsomz.ru/electro/electrol4/"
try:
    html=fetch(page)[0].decode("utf8","replace")
    links=sorted(set(re.findall(r"/i/splash_l4/20\d{6}-\d{4}\.jpg",html)))
    print("Electro HTML latest splash",links[-3:],flush=True)
    for term in ("Download","скачать","jpg","download","quality","resolution","splash_l4"):
        idx=html.lower().find(term.lower())
        if idx!=-1:print("Electro HTML snippet "+term,repr(html[max(0,idx-100):idx+280])[:380],flush=True)
    if links:test("electro_l4_preview",urllib.parse.urljoin(page,links[-1]))
except Exception as e:print("Electro visual test failed",str(e),flush=True)
pathlib.Path("satellite-image-quality-results.json").write_text(
 json.dumps({"time":now.isoformat(),"results":results},ensure_ascii=False,indent=2),encoding="utf8")
