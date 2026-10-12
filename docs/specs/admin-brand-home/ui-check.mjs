import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const cwd=fileURLToPath(new URL('../../../crates/ax-web/web-ui/',import.meta.url));
const server=spawn('node',['node_modules/vite/bin/vite.js','preview','--host','127.0.0.1','--port','7072','--strictPort'],{cwd,stdio:'ignore'});
let serverError;server.on('error',e=>{serverError=e;});
try {
let ready=false;
for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:7072',{signal:AbortSignal.timeout(1000)})).ok){ready=true;break;}}catch{}await new Promise(r=>setTimeout(r,100));}
if(serverError)throw serverError;
if(!ready)throw Error('Vite preview did not become ready');
const child=spawn('npm',['exec','--','playwright','test','e2e/admin-brand-home.spec.ts','e2e/control-spacing.spec.ts','--project=desktop-chrome','--project=mobile-chrome',...(process.env.AX_BRAND_GREP?['--grep',process.env.AX_BRAND_GREP]:[])],{cwd,env:{...process.env,AX_WEB_URL:'http://127.0.0.1:7072',AX_CHROME_EXECUTABLE:process.env.AX_CHROME_EXECUTABLE},stdio:'inherit'});
process.exitCode=await new Promise(r=>{child.on('error',()=>r(1));child.on('exit',code=>r(code??1));});
}finally{server.kill();}
