import json,sys,itertools
def lin(c):
    c/=255; return c/12.92 if c<=0.04045 else ((c+0.055)/1.055)**2.4
def oklab(h):
    h=h.lstrip('#'); r,g,b=[lin(int(h[i:i+2],16)) for i in (0,2,4)]
    l=(0.4122214708*r+0.5363325363*g+0.0514459929*b)**(1/3); m=(0.2119034982*r+0.6806995451*g+0.1073969566*b)**(1/3); s=(0.0883024619*r+0.2817188376*g+0.6299787005*b)**(1/3)
    return (0.2104542553*l+0.7936177850*m-0.0040720468*s, 1.9779984951*l-2.4285922050*m+0.4505937099*s, 0.0259040371*l+0.7827717662*m-0.8086757660*s)
def de(a,b):
    A,B=oklab(a),oklab(b); return sum((x-y)**2 for x,y in zip(A,B))**.5
PAL=dict(red='#c74240',orange='#dc813b',yellow='#f7d974',lime='#91ca55',green='#4ca167',teal='#217274',cyan='#6eccec',blue='#2e65b9',purple='#693d8c',magenta='#cd67b7',pink='#f7b8c6',brown='#6e4223',black='#202225',gray='#7b7d81',silver='#abaeb2',white='#f4f3f0')
