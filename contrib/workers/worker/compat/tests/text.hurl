# Plain text, CSV, logs, charsets, binary, compressed bodies and response headers.

GET {{origin}}/text/plain
HTTP 200
Content-Type: text/plain; charset=utf-8
```
Hello World!
Second line
```

GET {{origin}}/text/plain
HTTP 200
[Asserts]
body startsWith "Hello"
body endsWith "line\n"
body contains "Second"
body matches /^Hello \w+!/
body split "\n" count == 3
body split "\n" nth 1 == "Second line"
bytes count == 25

GET {{origin}}/text/unicode
HTTP 200
[Asserts]
body contains "日本語"
body contains "🚀"
regex /accents (\S+)/ == "àéîõü"

GET {{origin}}/text/latin1
HTTP 200
[Asserts]
header "Content-Type" == "text/plain; charset=ISO-8859-1"
body == "café crème"
bytes count == 10

GET {{origin}}/text/csv
HTTP 200
[Asserts]
header "Content-Type" == "text/csv"
body split "\n" nth 2 split "," nth 1 == "pear"
body split "\n" count == 4

GET {{origin}}/text/log
HTTP 200
[Asserts]
regex /code=(E\d+)/ == "E42"
regex /ms=(\d+)/ toInt > 800
body matches /WARN\s+slow/
body not contains "DEBUG"

GET {{origin}}/text/empty
HTTP 200
[Asserts]
body isEmpty
bytes count == 0

GET {{origin}}/text/bytes
HTTP 200
Content-Type: application/octet-stream
hex,000102feff;

HEAD {{origin}}/text/plain
HTTP 200
[Asserts]
body isEmpty

GET {{origin}}/text/headers
HTTP 200
[Asserts]
header "X-Single" == "one"
header "x-single" == "one"
header "Cache-Control" == "no-cache, max-age=0"
header "X-Absent" not exists
cookie "c1" == "v1"
cookie "c2" == "v2"
cookie "c2[Path]" == "/text"
cookie "c2[Max-Age]" == 3600
cookie "c2[HttpOnly]" exists
cookie "c3[Secure]" exists
cookie "c3[Expires]" dateFormat "%Y-%m-%d" == "2077-01-13"
