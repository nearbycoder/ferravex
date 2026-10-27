#!/usr/bin/env python3
"""Ferravex static host: stdlib, ETags, gzip, byte ranges, and Railway health checks."""
import argparse, functools, hashlib, io, mimetypes, os, re, shutil
from http import HTTPStatus
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

class Handler(SimpleHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    extensions_map={**SimpleHTTPRequestHandler.extensions_map,'.wasm':'application/wasm','.js':'text/javascript','.css':'text/css'}
    def setup(self):
        super().setup();self.connection.settimeout(30)
    def list_directory(self,path):
        self.send_error(404);return None
    def send_head(self):
        self.remaining=None
        if urlsplit(self.path).path=='/healthz':
            body=b'Ferravex docs healthy\n';self.send_response(200);self.send_header('Content-Type','text/plain; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();return io.BytesIO(body)
        original=Path(self.translate_path(self.path))
        if original.is_dir():
            if not urlsplit(self.path).path.endswith('/'):
                self.send_response(308);parts=urlsplit(self.path);self.send_header('Location',parts.path+'/'+('?' + parts.query if parts.query else ''));self.send_header('Content-Length','0');self.end_headers();return None
            original=original/'index.html'
        if not original.is_file():return self.not_found()
        use_gzip='gzip' in self.headers.get('Accept-Encoding','') and not self.headers.get('Range') and original.with_name(original.name+'.gz').is_file()
        path=original.with_name(original.name+'.gz') if use_gzip else original
        stat=path.stat();etag='"'+hashlib.sha256((str(stat.st_mtime_ns)+':'+str(stat.st_size)).encode()).hexdigest()[:24]+'"'
        if self.headers.get('If-None-Match')==etag:
            self.send_response(304);self.send_header('ETag',etag);self.send_header('Vary','Accept-Encoding');self.end_headers();return None
        stream=path.open('rb');start=0;end=stat.st_size-1;range_header=self.headers.get('Range');partial=False
        if range_header:
            match=re.fullmatch(r'bytes=(\d*)-(\d*)',range_header)
            if not match or not any(match.groups()):stream.close();self.range_error(stat.st_size);return None
            a,b=match.groups()
            if a:start=int(a);end=min(int(b),end) if b else end
            else:start=max(0,stat.st_size-int(b))
            if start>end or start>=stat.st_size:stream.close();self.range_error(stat.st_size);return None
            partial=True;stream.seek(start);self.remaining=end-start+1
        self.send_response(206 if partial else 200)
        self.send_header('Content-Type',self.guess_type(str(original)))
        self.send_header('Content-Length',str(end-start+1));self.send_header('ETag',etag)
        self.send_header('Cache-Control','public, max-age=0, must-revalidate');self.send_header('Vary','Accept-Encoding');self.send_header('Accept-Ranges','bytes')
        self.send_header('X-Content-Type-Options','nosniff');self.send_header('Referrer-Policy','strict-origin-when-cross-origin')
        if use_gzip:self.send_header('Content-Encoding','gzip')
        if partial:self.send_header('Content-Range',f'bytes {start}-{end}/{stat.st_size}')
        self.end_headers();return stream
    def not_found(self):
        # Missing paths keep their 404 status but show the site's own recovery page.
        page=Path(self.directory)/'404.html'
        if not page.is_file():self.send_error(404);return None
        body=page.read_bytes();self.send_response(404);self.send_header('Content-Type','text/html; charset=utf-8')
        self.send_header('Content-Length',str(len(body)));self.send_header('Cache-Control','no-cache')
        self.send_header('X-Content-Type-Options','nosniff');self.send_header('Referrer-Policy','strict-origin-when-cross-origin')
        self.end_headers();return io.BytesIO(body)
    def range_error(self,size):
        self.send_response(416);self.send_header('Content-Range',f'bytes */{size}');self.send_header('Content-Length','0');self.end_headers()
    def copyfile(self,source,outputfile):
        if self.remaining is None:return shutil.copyfileobj(source,outputfile)
        while self.remaining:
            chunk=source.read(min(self.remaining,64*1024))
            if not chunk:break
            outputfile.write(chunk);self.remaining-=len(chunk)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--port',type=int,default=int(os.environ.get('PORT','8080')));parser.add_argument('--bind',default='0.0.0.0');parser.add_argument('--directory',type=Path,default=Path(__file__).resolve().parent.parent/'dist/site');args=parser.parse_args()
    if not 1<=args.port<=65535:parser.error('port must be in 1..65535')
    if not (args.directory/'index.html').is_file():parser.error('build dist/site with scripts/build-site.py first')
    server=ThreadingHTTPServer((args.bind,args.port),functools.partial(Handler,directory=str(args.directory.resolve())))
    print(f'Ferravex docs listening on {args.bind}:{args.port}',flush=True)
    try:server.serve_forever()
    except KeyboardInterrupt:pass
    finally:server.server_close()
if __name__=='__main__':main()
