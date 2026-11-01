from playwright.sync_api import sync_playwright
import os
with sync_playwright() as p:
    b=p.chromium.launch()
    for n,w,h in [("floating-menu",960,600),("settings",1200,780),("tray",720,520)]:
        pg=b.new_page(viewport={"width":w,"height":h},device_scale_factor=2)
        pg.goto("file://"+os.path.abspath(n+".html")); pg.wait_for_timeout(300)
        pg.screenshot(path=n+".png")
