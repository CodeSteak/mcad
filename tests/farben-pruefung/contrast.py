import re,sys
css=open('/home/user/controller-cad/web/src/styles.css').read()
def block(t):
    m=re.search(r':root\[data-theme="%s"\]\s*\{([^}]*)\}'%t,css); return dict(re.findall(r'--([\w-]+):\s*([^;]+);',m.group(1)))
def rgb(v,bg=None):
    v=v.strip()
    if v.startswith('#'): return [int(v[i:i+2],16) for i in (1,3,5)]
    m=re.match(r'rgba\(([^)]*)\)',v); r,g,b,a=[float(x) for x in m.group(1).split(',')]; return [r*a+bb*(1-a) for r,bb in zip((r,g,b),bg)]
def L(c):
    f=lambda x:(x/255)/12.92 if x/255<=0.04045 else ((x/255+0.055)/1.055)**2.4
    r,g,b=map(f,c); return 0.2126*r+0.7152*g+0.0722*b
def cr(a,b):
    la,lb=sorted([L(a),L(b)],reverse=True); return (la+0.05)/(lb+0.05)
for t in ('light','dark'):
    k=block(t); print('==',t)
    for fg,bg in [('text','bg'),('text','panel'),('muted','panel'),('muted','panel-2'),('muted','bg'),('accent','panel'),('error','panel'),('warning','panel'),('hl-comment','panel'),('hl-operator','panel'),('view-edge','view-bg'),('view-outline','view-bg'),('view-model','view-bg'),('view-grid','view-bg'),('view-anchor','view-bg'),('border','panel'),('on-accent','accent')]:
        b=rgb(k[bg]); f=rgb(k[fg],b); print(f'  {fg:13s} on {bg:9s} {cr(f,b):5.2f}')
    b=rgb(k['panel']); wb=rgb(k['warning-bg'],b); print('  warning on warning-bg', round(cr(rgb(k['warning']),wb),2)); eb=rgb(k['error-bg'],b); print('  error on error-bg', round(cr(rgb(k['error']),eb),2))
