# Response bodies by content type, charset and Content-Encoding. The bodies are decoded by the
# same Rust code in both builds; only the transport differs (gzip/br: see known_difference_gzip.hurl).

GET {{origin}}/content/deflate
HTTP 200
[Asserts]
body == "Hello encoded world! café 日本\n"
sha256 == hex,89e71f16c4c1a4896261a351924097e63225d25d0a3175f1cf0b40f9f2e2d317;

GET {{origin}}/content/identity
HTTP 200
[Asserts]
body == "Hello encoded world! café 日本\n"
sha256 == hex,89e71f16c4c1a4896261a351924097e63225d25d0a3175f1cf0b40f9f2e2d317;

GET {{origin}}/content/json-vendor
HTTP 200
[Asserts]
jsonpath "$.data[0].id" == 1

GET {{origin}}/content/ndjson
HTTP 200
[Asserts]
body split "\n" count == 3

GET {{origin}}/content/sse
HTTP 200
[Asserts]
body contains "data: 2"
regex "data: (\\d)" == "1"

GET {{origin}}/content/png
HTTP 200
[Asserts]
bytes count == 16
bytes startsWith hex,89504e47;

GET {{origin}}/content/multipart-mixed
HTTP 200
[Asserts]
body contains "part1"

GET {{origin}}/content/html-octet
HTTP 200
[Asserts]
xpath "string(//p)" == "x"

GET {{origin}}/content/no-content-type
HTTP 200
[Asserts]
body == "plain without type"

GET {{origin}}/content/windows1252
HTTP 200
[Asserts]
body == "café €"

GET {{origin}}/content/shiftjis
HTTP 200
[Asserts]
body == "日本"
