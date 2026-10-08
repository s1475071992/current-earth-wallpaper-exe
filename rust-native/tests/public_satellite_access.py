"""Public no-login satellite access probe for selecting future wallpaper sources.
Safe: GET only, timeout-bound, no secrets, no current wallpaper modification.
"""
import datetime as dt, io, json, re, urllib.error, urllib.parse, urllib.request
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from pathlib import Path

HEADERS = {"User-Agent":"CurrentEarthWallpaper-source-evaluation/1.2 (+public satellite image probe)",
           "Accept":"image/avif,image/webp,image/png,image/jpeg,image/*;q=0.9,*/*;q=0.5"}
now = dt.datetime.now(dt.timezone.utc)
output = {"checked_utc":now.isoformat(),"results":[]}
def add(name, state, **info):
    output["results"].append({"name":name,"state":state,**info})
    print(name, state, json.dumps(info,ensure_ascii=False)[:700],flush=True)
def fetch(url, limit=1100000, timeout=14):
    req=urllib.request.Request(url,headers=HEADERS)
    with urllib.request.urlopen(req,timeout=timeout) as r:
        data=r.read(limit+1)
        return {"status":r.status,"url":r.geturl(),"content_type":r.headers.get("Content-Type",""),
                "content_length":r.headers.get("Content-Length",""),"bytes_read":len(data),
                "sample":data[:180],"data":data}
def kind(data):
    if data.startswith(b"\xff\xd8\xff"):return "JPEG"
    if data.startswith(b"\x89PNG\r\n\x1a\n"):return "PNG"
    if data.startswith(b"GIF8"):return "GIF"
    if data.startswith(b"<?xml") or data.startswith(b"<ServiceException"):return "XML-error"
    if data.startswith(b"\x89HDF") or data[:3]==b"CDF":return "NetCDF"
    return "not-recognized-image"
def check(name,url):
    try:
        r=fetch(url)
        tp=kind(r["data"])
        add(name,"IMAGE_NO_AUTH" if tp in ("JPEG","PNG","GIF") and r["bytes_read"]>5000 else "NO_USABLE_IMAGE",
            status=r["status"],mime=r["content_type"],image_type=tp,bytes=r["bytes_read"],url=r["url"],
            peek=r["sample"][:75].decode("utf8","replace") if tp not in ("JPEG","PNG","GIF") else "")
    except Exception as e: add(name,"REQUEST_FAILED",url=url,error=str(e)[:230])

base="https://view.eumetsat.int/geoserver/wms"
q={"SERVICE":"WMS","VERSION":"1.1.1","REQUEST":"GetMap",
   "STYLES":"","FORMAT":"image/jpeg","SRS":"EPSG:4326",
   "BBOX":"-29.5,-75,120.5,75","WIDTH":"700","HEIGHT":"700"}
for layer in ("msg_iodc:rgb_naturalenhncd","msg_iodc:rgb_natural"):
    q["LAYERS"]=layer
    check("Meteosat9_WMS_"+layer,base+"?"+urllib.parse.urlencode(q))

q.update({"LAYERS":"mtg_fd:rgb_geocolour","BBOX":"-77,-77,77,77"})
check("Existing_Meteosat12_WMS_baseline",base+"?"+urllib.parse.urlencode(q))

# NMSC public /IMG URLs (not the API key protected download service).
host="https://nmsc.kma.go.kr/IMG/GK2A/AMI/PRIMARY/L1B/COMPLETE/FD"
times=[]
for lag in [45,75,105,165,245,390]:
    t=now-dt.timedelta(minutes=lag)
    t=t.replace(minute=(t.minute//10)*10,second=0,microsecond=0)
    if t not in times: times.append(t)
for i,t in enumerate(times):
    stamp=t.strftime("%Y%m%d%H%M")
    url=f"{host}/{t:%Y%m}/{t:%d}/{t:%H}/gk2a_ami_le1b_rgb-true_fd010ge_{stamp}.srv.png"
    try:
        r=fetch(url,350000,8)
        tp=kind(r["data"])
        if tp=="PNG":
            add("GK2A_KMA_public_image","IMAGE_NO_AUTH",timestamp=stamp,bytes=r["bytes_read"],url=url)
            break
        if i==len(times)-1:add("GK2A_KMA_public_image","NOT_CONFIRMED",tries=len(times),last_http=r["status"],last_kind=tp)
    except Exception as e:
        if i==len(times)-1:add("GK2A_KMA_public_image","NOT_CONFIRMED",tries=len(times),last_error=str(e)[:160])

# S3, explicitly anonymous GET and no AWS key.
s3="https://noaa-gk2a-pds.s3.amazonaws.com/"
try:
    r=fetch(s3+"?list-type=2&delimiter=%2F&max-keys=30",limit=150000)
    root=ET.fromstring(r["data"])
    def val(el,n):return el.findtext("{*}"+n) or ""
    prefixes=[val(cp,"Prefix") for cp in root.findall("{*}CommonPrefixes")]
    keys=[val(k,"Key") for k in root.findall("{*}Contents")]
    add("GK2A_NOAA_anonymous_S3","LIST_NO_AUTH" if r["status"]==200 else "NOT_CONFIRMED",prefixes=prefixes[:25],keys=keys[:8],body_bytes=r["bytes_read"])
    chosen=[p for p in prefixes if any(x in p.lower() for x in ["2026","gk2a","ami","rgb","full","fd","level1"])]
    chosen=(chosen or prefixes)[:2]
    for depth in range(4):
        if not chosen:break
        nextp=[]
        for prefix in chosen[:2]:
            url=s3+"?"+urllib.parse.urlencode({"list-type":"2","delimiter":"/","prefix":prefix,"max-keys":30})
            rr=fetch(url,150000)
            tree=ET.fromstring(rr["data"])
            ps=[val(cp,"Prefix") for cp in tree.findall("{*}CommonPrefixes")]
            ks=[val(k,"Key") for k in tree.findall("{*}Contents")]
            add("GK2A_S3_tree", "FOUND",depth=depth+1,prefix=prefix,prefixes=ps[:8],keys=ks[:6])
            nextp.extend(ps[:2])
        chosen=nextp[:2]
except Exception as e:add("GK2A_NOAA_anonymous_S3","REQUEST_FAILED",error=str(e)[:220])

# Russian publicly viewable thumbnails: assess recency and direct no-login HTTPS retrieval.
class ImgFinder(HTMLParser):
    def __init__(self):super().__init__();self.src=[]
    def handle_starttag(self,tag,attrs):
        if tag.lower()=="img":
            x=dict(attrs);self.src.append(x.get("src",""))
for name, page in [("ElektroL3","https://electro.ntsomz.ru/electro/electrol3/"),
                   ("ElektroL4","https://electro.ntsomz.ru/electro/electrol4/")]:
    try:
        r=fetch(page,limit=1000000,timeout=16)
        html=r["data"].decode("utf8","replace")
        urls=re.findall(r"(?:(?:https?://)?[^\"'\s<>]*)?/i/(?:splash|splash_l[234])/[0-9]{8}-[0-9]{4}\.jpg",html)
        urls=sorted(set(urls))
        p=ImgFinder();p.feed(html)
        urls += [v for v in p.src if "/i/splash" in v and v not in urls]
        dates=re.findall(r"(20\d{6})-\d{4}",str(urls))
        newest=max(dates) if dates else ""
        add(name+"_index","OPEN_NO_AUTH",status=r["status"],image_links=len(urls),newest_image_date=newest,html_bytes=len(r["data"]),samples=urls[-3:])
        if urls:
            img=urllib.parse.urljoin(page,urls[-1])
            check(name+"_jpg",img)
    except Exception as e:add(name+"_index","REQUEST_FAILED",error=str(e)[:200])

check("FY4C_guessed_public_CDN","https://img.nsmc.org.cn/CLOUDIMAGE/FY4C/AGRI/GCLR/FY4C_DISK_GCLR.JPG")
check("MOSDAC_INSAT_public_viewer_only","https://www.mosdac.gov.in/live/lite/index.html")

p=Path("satellite-source-access-results.json")
p.write_text(json.dumps(output,ensure_ascii=False,indent=2),encoding="utf-8")
print("wrote",p,len(output["results"]),"checks",flush=True)
