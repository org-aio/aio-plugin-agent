// 本地夹具的宿主实现导航契约；正式环境由 AIO 壳提供同一消息接口。
export function previewNavigation() {
  return `const frame=document.querySelector('iframe');
  const navigation=fragment=>frame.contentWindow.postMessage({channel:'aio-navigation',token:'preview',navigation:fragment},'*');
  frame.addEventListener('load',()=>navigation(new URLSearchParams(location.search).get('route')||''));
  addEventListener('popstate',()=>navigation(new URLSearchParams(location.search).get('route')||''));
  addEventListener('message',event=>{const m=event.data;if(event.source!==frame.contentWindow||m?.channel!=='aio-navigation'||m.token!=='preview')return;
    if(typeof m.navigation==='string'){const url=new URL(location.href);url.searchParams.set('route',m.navigation);history[m.replace?'replaceState':'pushState']({},'',url);navigation(m.navigation);}
    frame.contentWindow.postMessage({channel:'aio-navigation',token:'preview',id:m.id,response:null},'*');
  });`;
}
