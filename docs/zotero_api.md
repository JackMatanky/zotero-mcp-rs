# Zotero Web API v3

Source: [Zotero Web API v3](https://www.zotero.org/support/dev/web_api/v3)
GitHub: [zotero/zotero-docs](https://github.com/zotero/zotero-docs)

# Zotero Web API Documentation

This page documents read requests available in the [Zotero Web API](dev/web_api/v3/), providing read-only access to online Zotero libraries. The Zotero desktop client exposes the same endpoints as a [local API](dev/web_api/v3/local_api) that serves data from the local database, which is useful for code running on the user's computer.

## Base URL

The base URL for all API requests is

    https://api.zotero.org

All requests must use HTTPS. (The local API uses `http://localhost:23119/api/` instead; see [Local API](dev/web_api/v3/local_api).)

## API Versioning

Multiple versions of the API are available, and production code should always request a specific version. This page documents API version 3, which is currently the default and recommended version.

Clients can request a specific version in one of two ways:

1.  Via the `Zotero-API-Version` HTTP header (`Zotero-API-Version: 3`)
2.  Via the `v` query parameter (`v=3`)

Use of the `Zotero-API-Version` header is recommended for production code. The `v` parameter can be used for easier debugging and sharing of API requests or in clients that can't pass arbitrary HTTP headers.

The API version of a response will be returned in the `Zotero-API-Version` response header.

### Version Transitions

At most times, API changes are made in a backwards-compatible manner. Occasionally, however, backwards-incompatible changes may need to be made. When this occurs, a new API version will be made available without changing the default version for clients that don't request a specific version. After a transition period, the new API version will become the default. If an API version is discontinued, clients requesting the discontinued version will receive the oldest available version. Announcements regarding API version transitions will always be made ahead of time on [zotero-dev](https://groups.google.com/group/zotero-dev).

## Authentication

Authentication is not required for read access to public libraries.

Accessing non-public libraries requires use of an API key. Third-party developers should [use OAuth to establish credentials](dev/web_api/v3/oauth) or instruct their users to create dedicated keys for use with their services. End users can create API keys via [their Zotero account settings](/settings/keys/new).

API keys can be included in requests in one of three ways:

1.  As an HTTP header in the form `Zotero-API-Key: P9NiFoyLeZu2bZNvvuQPDWsd`
2.  As an HTTP header in the form `Authorization: Bearer P9NiFoyLeZu2bZNvvuQPDWsd`
3.  As a URL query parameter, in the form `key=P9NiFoyLeZu2bZNvvuQPDWsd` (not recommended)

Use of an HTTP header is recommended, as it allows use of URLs returned from the API (e.g., for pagination) without modification.

`Authorization: Bearer` is also the authentication mechanism for OAuth 2.0. While Zotero currently supports only OAuth 1.0a, when support for OAuth 2.0 is added, clients will no longer need to extract the API key from the OAuth response and pass it to the API separately.

The [local API](dev/web_api/v3/local_api) does not use authentication for read requests. Write requests (Zotero 10+) require a local API key, which is unrelated to a zotero.org API key and is granted by the user at runtime; see [Authorizing Writes](dev/web_api/v3/local_api#authorizing_writes).

## Local API

The Zotero desktop client exposes a local implementation of this API on `localhost:23119` under `/api/`, serving data from the user's local database. See [Local API](dev/web_api/v3/local_api) for details. The sections below note where local API behavior diverges from the Web API.

## Resources

### User and Group Library URLs

Requests for data in a specific library begin with `/users/<userID>` or `/groups/<groupID>`, referred to below as <userOrGroupPrefix>. User IDs are different from usernames and can be found on the [API Keys](/settings/keys) page and in OAuth responses. Group IDs are different from group names and can be retrieved from `/users/<userID>/groups`.

#### Collections

| URI | Description |
| :--- | :--- |
| `<userOrGroupPrefix>`/collections | Collections in the library |
| `<userOrGroupPrefix>`/collections/top | Top-level collections in the library |
| `<userOrGroupPrefix>`/collections/`<collectionKey>` | A specific collection in the library |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/collections | Subcollections within a specific collection in the library |

#### Items

| URI | Description |
| :--- | :--- |
| `<userOrGroupPrefix>`/items | All items in the library, excluding trashed items |
| `<userOrGroupPrefix>`/items/top | Top-level items in the library, excluding trashed items |
| `<userOrGroupPrefix>`/items/trash | Items in the trash |
| `<userOrGroupPrefix>`/items/`<itemKey>` | A specific item in the library |
| `<userOrGroupPrefix>`/items/`<itemKey>`/children | Child items under a specific item |
| `<userOrGroupPrefix>`/publications/items | Items in My Publications |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/items | Items within a specific collection in the library |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/items/top | Top-level items within a specific collection in the library |

#### Searches

(Note: Only search metadata is currently available, not search results.)

| URI | Description |
| :--- | :--- |
| `<userOrGroupPrefix>`/searches | All saved searches in the library |
| `<userOrGroupPrefix>`/searches/`<searchKey>` | A specific saved search in the library |

#### Tags

| URI | Description |
| :--- | :--- |
| `<userOrGroupPrefix>`/tags | All tags in the library |
| `<userOrGroupPrefix>`/tags/<url+encoded+tag> | Tags of all types matching a specific name |
| `<userOrGroupPrefix>`/items/`<itemKey>`/tags | Tags associated with a specific item |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/tags | Tags within a specific collection in the library |
| `<userOrGroupPrefix>`/items/tags | All tags in the library, with the ability to filter based on the items |
| `<userOrGroupPrefix>`/items/top/tags | Tags assigned to top-level items |
| `<userOrGroupPrefix>`/items/trash/tags | Tags assigned to items in the trash |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/items/tags | Tags assigned to items in a given collection |
| `<userOrGroupPrefix>`/collections/`<collectionKey>`/items/top/tags | Tags assigned to top-level items in a given collection |
| `<userOrGroupPrefix>`/publications/items/tags | Tags assigned to items in My Publications |

### Other URLs

| URI | Description |
| :--- | :--- |
| `/keys/<key>` | The user id and privileges of the given API key.<br>Use the `DELETE` HTTP method to delete the key. This should generally be done only by a client that created the key originally using [OAuth](/dev/web_api/v3/OAuth). |
| `/users/<userID>/groups` | The set of groups the current API key has access to, including public groups the key owner belongs to even if the key doesn't have explicit permissions for them. |

## Read Requests

The following parameters affect the format of data returned from read requests. All parameters are optional.

### General Parameters

The following parameters are valid for all read requests:

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `format` | `atom`, `bib`, `json`, `keys`, `versions`, [export format](#export_formats) | `json` (or `atom` if the `Accept` header includes `application/atom+xml`) | `atom` will return an Atom feed suitable for use in feed readers or feed-reading libraries.<br>`bib`, valid only for item requests, will return a formatted bibliography as XHTML. `bib` mode is currently limited to a maximum of 150 items.<br>`json` will return a JSON array for multi-object requests and a single JSON object for single-object requests.<br>`keys`, valid for multi-object requests, will return a newline-separated list of object keys. `keys` mode has no default or maximum limit.<br>`versions`, valid for multi-object collection, item, and search requests, will return a JSON object with Zotero object keys as keys and object versions as values. Like `keys`, `versions` mode has no default or maximum limit.<br>Export formats, valid only for item requests, produce output in the specified format. |

The [local API](dev/web_api/v3/local_api) does not support `format=atom`; requests using it return `501 Not Implemented`. All other formats are supported.

### Parameters for "format=json"

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `include` | `bib`, `citation`, `data`, [export format](#export_formats)<br>Multiple formats can be specified by using a comma as the delimiter (`include=data,bib`). | `data` | Formats to include in the response:<br>`bib`, valid only for item requests, will return a formatted reference for each item.<br>`citation`, valid only for item requests, will return a formatted citation for each item.<br>`data` (the default) will include all writeable fields in JSON format, suitable for modifying and sending back to the API.<br>Export formats, valid only for item requests, will return data in the specified format for each item. |

### Parameters for "format=atom"

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `content` | `bib`, `citation`, `html`, `json`, [export formats](#export_formats), `none`<br>Multiple formats can be specified by using a comma as the delimiter (`content=json,bib`). | `html` | The format of the Atom response's `<content>` node:<br>`html` (the default) will return an XHTML representation of each object, useful for display in feed readers and for parsing by XML tools.<br>`json`, currently valid only for item and collection requests, will return a JSON representation of all the item's fields.<br>`bib`, valid only for item requests, will return a formatted reference for each item.<br>`citation`, valid only for item requests, will return a formatted citation for each item.<br>Export formats, valid only for item requests, will return data in the specified format for each item.<br>If additional data is not required, use `none` to decrease the response size.<br>If multiple formats are requested, `<content>` will contain multiple `<zapi:subcontent>` elements (in the http://zotero.org/ns/api namespace), each with a `zapi:type` attribute matching one of the specified content parameters. |

The Atom parameters above do not apply to the [local API](dev/web_api/v3/local_api), which does not produce Atom output.

### Parameters for "format=bib", "include/content=bib", "include/content=citation"

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `style` | string | `chicago-note-bibliography` | Citation style to use for formatted references. Can be either the file name (without the .csl extension) of one of the styles in the [Zotero Style Repository](https://www.zotero.org/styles/) (e.g., `apa`) or the URL of a remote CSL file. |
| `linkwrap` | boolean | `0` | Set to `1` to return URLs and DOIs as links |
| `locale` | string | `en-US` | Bibliography locale. See the [available CSL locales](https://github.com/citation-style-language/locales). Note that some styles use a fixed locale and cannot be localized. |

Note the difference between `format=bib` and `include=bib`/`content=bib`. `format=bib` returns a formatted bibliography as XHTML, sorted according to the rules of the selected style. `include=bib` (valid only for `format=json` (the default format mode) and `format=atom`) returns an individual formatted reference within the JSON `data` block or Atom <content> block for each item, with the results or feed sorted according to the query parameters. `format=bib` processes the entire feed you are requesting without regard for any limit arguments, so it is generally a good idea to use it only with collections or tags.

### Item Export Formats

The following bibliographic data formats can be used as `format`, `include`, and `content` parameters for items requests:

| Parameter | Description |
| :--- | :--- |
| `bibtex` | BibTeX |
| `biblatex` | BibLaTeX |
| `bookmarks` | Netscape Bookmark File Format |
| `coins` | COinS |
| `csljson` | [Citation Style Language data format](https://github.com/citation-style-language/schema/blob/master/csl-data.json) |
| `csv` | CSV |
| `mods` | MODS |
| `refer` | Refer/BibIX |
| `rdf_bibliontology` | [Bibliographic Ontology](http://bibliontology.com/) RDF |
| `rdf_dc` | Unqualified Dublin Core RDF |
| `rdf_zotero` | Zotero RDF |
| `ris` | RIS |
| `tei` | Text Encoding Initiative (TEI) |
| `wikipedia` | Wikipedia Citation Templates |

## Searching

### Search Parameters

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `itemKey` | string | null | A comma-separated list of item keys. Valid only for item requests. Up to 50 items can be specified in a single request. |
| `itemType` | [search syntax](#search_syntax) | null | Item type search |
| `q` | string | null | Quick search. Searches titles and individual creator fields by default. Use the `qmode` parameter to change the mode. Currently supports phrase searching only. |
| `since` | integer | `0` | Return only objects modified after the specified library version, returned in a previous `Last-Modified-Version` header. See [Syncing](dev/web_api/v3/syncing) for more info. |
| `tag` | [search syntax](#search_syntax) | null | Tag search |

The [local API](dev/web_api/v3/local_api) accepts the same search parameters but uses Zotero's local quicksearch implementation, so the set of items returned by a given `q` value may not match the Web API exactly.

### Search Parameters (Items Endpoints)

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `includeTrashed` | `0`, `1` | `0` (except in `/trash`) | Include items in the trash |
| `qmode` | `titleCreatorYear`, `everything` | `titleCreatorYear` | Quick search mode. To include full-text content, use `everything`. Searching of other fields will be possible in the future. |

### Search Parameters (Tags Endpoints)

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `qmode` | `contains`, `startsWith` | `contains` | Quick search mode. To perform a left-bound search, use `startsWith`. |

### Search Parameters (Tags-Within-Items Endpoints)

These parameters can be used to search against items when returning tags within items. In such cases, the main parameters (`q`, `qmode`, `tag`) apply to the tags themselves, as the primary objects of the request.

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `itemQ` | string | null | Same as `q` in an `items` request |
| `itemQMode` | `contains`, `startsWith` | `contains` | Same as `qmode` in an `items` request |
| `itemTag` | [search syntax](#search_syntax) | null | Same as `tag` in an `items` request |

### Search Syntax

`itemType` and `tag` parameters support Boolean searches:

Examples:

- `itemType=book`
- `itemType=book || journalArticle` (OR)
- `itemType=-attachment` (NOT)
- `tag=foo`
- `tag=foo bar` (tag with space)
- `tag=foo&tag=bar` (AND)
- `tag=foo bar || bar` (OR)
- `tag=-foo` (NOT)
- `tag=\-foo` (literal first-character hyphen)

Be sure to URL-encode search strings if required by your client or library.

## Sorting and Pagination

### Sorting and Pagination Parameters

The following parameters are valid only for multi-object read requests such as `<userOrGroupPrefix>/items`, with the exception of `format=bib` requests, which do not support sorting or pagination.

| Parameter | Values | Default | Description |
| :--- | :--- | :--- | :--- |
| `sort` | `dateAdded`, `dateModified`, `title`, `creator`, `itemType`, `date`, `publisher`, `publicationTitle`, `journalAbbreviation`, `language`, `accessDate`, `libraryCatalog`, `callNumber`, `rights`, `addedBy`, `numItems` (tags) | `dateModified` (`dateAdded` for Atom) | The name of the field by which entries are sorted |
| `direction` | `asc`, `desc` | varies by `sort` | The sorting direction of the field specified in the `sort` parameter |
| `limit` | integer 1-100\* | `25` | The maximum number of results to return with a single request. Required for export formats. |
| `start` | integer | `0`    | The index of the first result. Combine with the limit parameter to select a slice of the available results. |

The [local API](dev/web_api/v3/local_api) does not impose a default or maximum `limit`. If `limit` is omitted, all matching objects are returned in one response. Pagination parameters and `Link` headers still work for clients that want them.

#### Total Results

Responses for multi-object read requests will include a custom HTTP header, `Total-Results`, that provides the total number of results matched by the request. In the Web API, the actual number of results provided in a given response will be no more than 100.

#### Link Header

When the total number of results matched by a read request is greater than the current limit, the API will include pagination links in the HTTP `Link` header. Possible values are `rel=first`, `rel=prev`, `rel=next`, and `rel=last`. For some requests, the header may also include a `rel=alternate` link for the relevant page on the Zotero website.

    GET https://api.zotero.org/users/12345/items?limit=30

    Link: <https://api.zotero.org/users/12345/items?limit=30&start=30>; rel="next",
     <https://api.zotero.org/users/12345/items?limit=30&start=5040>; rel="last",
     <https://www.zotero.org/users/12345/items>; rel="alternate"

(Newlines are inserted here for clarity.)

## Caching

For efficient usage of the API, clients should make conditional GET requests whenever possible. Multi-object requests (e.g., `/users/1/items`) return a `Last-Modified-Version` header with the current library version. If a `If-Modified-Since-Version: <libraryVersion>` header is passed with a subsequent multi-object read request and data has not changed in the library since the specified version, the API will return `304 Not Modified` instead of `200`. (Single-object conditional requests are not currently supported, but will be supported in the future.)

While a conditional GET request that returns a `304` should be fast, some clients may wish or need to perform additional caching on their own, using stored data for a period of time before making subsequent conditional requests to the Zotero API. This makes particular sense when the underlying Zotero data is known not to change frequently or when the data will be accessed frequently. For example, a website that displayed a bibliography from a Zotero collection might cache the returned bibliography for an hour, after which time it would make another conditional request to the Zotero API. If the API returned a `304`, the website would continue to display the cached bibliography for another hour before retrying. This would prevent the website from making a request to the Zotero API every time a user loaded a page.

In addition to making conditional requests, clients downloading data for entire Zotero libraries should use `?since=` to request only objects that have changed since the last time data was downloaded.

See [Syncing](dev/web_api/v3/syncing) for more information on library and object versioning.

Conditional requests work the same way against the [local API](dev/web_api/v3/local_api), but local responses are already inexpensive to produce, so aggressive caching on the client side is less important. The `?since=` parameter is also supported and is the preferred way to fetch only changed objects from a large local library. Note that in Zotero 10+, local versions are unrelated to the versions in this API, so clients must partition cached data by server ID. See [Object Versions](dev/web_api/v3/local_api#object_versions).

## Rate Limiting

Clients accessing the Zotero API should be prepared to handle two forms of rate limiting: backoff requests and hard limiting.

### Backoff

If the API servers are overloaded, the API may include a `Backoff: <seconds>` HTTP header in responses, indicating that the client should perform the minimum number of requests necessary to maintain data consistency and then refrain from making further requests for the number of seconds indicated. `Backoff` can be included in any response, including successful ones.

### 429 Too Many Requests

If a client has made too many requests within a given time period or is making too many concurrent requests, the API may return `429 Too Many Requests`, potentially with a `Retry-After: <seconds>` header. Clients receiving a `429` should wait at least the number of seconds indicated in the header before making further requests, or to perform an exponential backoff if `Retry-After` isn't provided. They should also reduce their overall request rate and/or concurrency to avoid repeatedly getting 429s, which may result in stricter throttling or temporary blocks.

For clients repeatedly hitting rate limits, the best solution is often to use a different request flow, such as using [batched writes](dev/web_api/v3/write_requests#updating_multiple_objects) rather individually updating a large number of objects. Clients should also generally make no more than 4 concurrent requests.

`Retry-After` can also be included with `503 Service Unavailable` responses when the server is undergoing maintenance.

## Example GET Requests and Responses

Several examples of read request URLs and their responses:

| Description | Details |
| :--- | :--- |
| **Multi-object JSON response: top-level items in a collection** | **Request:** `https://api.zotero.org/users/475425/collections/9KH9TNSJ/items/top?v=3`<br>**Response:** `https://gist.github.com/6eeace93a5c29775d39c` |
| **Single-object JSON response: individual item** | **Request:** `https://api.zotero.org/users/475425/items/X42A7DEE?v=3`<br>**Response:** `https://gist.github.com/f1030b9609aadc51ddec` |
| **Multi-object JSON response: collections for a user** | **Request:** `https://api.zotero.org/users/475425/collections?v=3`<br>**Response:** `https://gist.github.com/0bc17ca64ee7d3bc9063` |
| **Atom feed: items in a library** | **Request:** `https://api.zotero.org/users/475425/items?format=atom&v=3`<br>**Response:** `https://gist.github.com/24172188ea79efa210b5` |
| **Formatted bibliography: items in a collection** | **Request:** `https://api.zotero.org/users/475425/collections/9KH9TNSJ/items?format=bib`<br>**Response:** `https://gist.github.com/77bc2413cce4c219f862` |

The [local API](dev/web_api/v3/local_api) does not currently apply rate limiting.

## HTTP Status Codes

Successful GET requests will return a `200 OK` status code.

[Conditional GET requests](#caching) may return `304 Not Modified`.

For any read or write request, the server may return a `400 Bad Request`, `404 Not Found`, or `405 Method Not Allowed` for an invalid request and `500 Internal Server Error` or `503 Service Unavailable` for a server-related issue. [Authentication](#authentication) errors (e.g., invalid API key or insufficient privileges) will return a `403 Forbidden`.

Passing an `Expect` header, which is unsupported, will result in a `417 Expectation Failed` response.

[Library/object versioning](dev/web_api/v3/syncing#version_numbers) or [Zotero-Write-Token](dev/web_api/v3/write_requests#zotero_write_token) errors will result in `412 Precondition Failed` or `428 Precondition Required`.

`429 Too Many Requests` indicates that the client has been [rate-limited](#rate_limiting).

The [local API](dev/web_api/v3/local_api) returns `403 Forbidden` when the local API preference is not enabled and `501 Not Implemented` when a request asks for something it does not support (Atom output, or an API version other than 3 on an endpoint other than `/api/`). It returns `401 Unauthorized` for a write request without a valid [local API key](dev/web_api/v3/local_api#authorizing_writes), and `412` or `428` for [`Zotero-Server-ID`](dev/web_api/v3/local_api#server_id) errors in addition to the versioning errors above.

## Additional API Documentation

-   [Local API](dev/web_api/v3/local_api)
-   [Write Requests](dev/web_api/v3/write_requests)
-   [File Uploads](dev/web_api/v3/file_upload)
-   [Syncing](dev/web_api/v3/syncing)
-   [OAuth Authentication](dev/web_api/v3/oauth)
-   [Changes from API Version 2](dev/web_api/v3/changes_from_v2)

# Zotero Local API

Recent versions of the Zotero desktop client expose a local implementation of the [Zotero Web API](dev/web_api/v3/basics) on `localhost:23119` under `/api/`, serving data from the user's local database. Because nothing touches the network, the local API works offline, has no rate limits, and is typically much faster than the Web API. It is intended for code running on the user's own computer, like utilities and command-line tools, that would otherwise need to read directly from the SQLite database or fall back to the Web API.

The local API must be enabled in Zotero's preferences (Settings → Advanced → "Allow other applications on this computer to communicate with Zotero"). Requests will return `403 Forbidden` if it is not enabled.

The base URL is

    http://localhost:23119/api/

Most endpoints documented on the [Basics](dev/web_api/v3/basics), [Write Requests](dev/web_api/v3/write_requests) (Zotero 10+), [File Uploads](dev/web_api/v3/file_upload), and [Full-Text Content](dev/web_api/v3/fulltext_content) pages work identically when accessed under that prefix. The notable differences from the Web API are:

-   Only API version 3 is supported, and only one version will ever be supported at a time. If a future version is released and your client needs to work against both old and new copies of Zotero, request `/api/` first and read the `Zotero-API-Version` response header to determine which version the running client speaks before making further requests.
-   Read requests require no authentication. Write requests (Zotero 10+) require a local API key, which the user grants through a confirmation dialog. See [Authorizing Writes](#authorizing_writes).
-   Since reads are unauthenticated, applications running locally can read the user's library. Do not forward the port or otherwise expose it externally.
-   In Zotero 10+, every response includes a `Zotero-Server-ID` header identifying the Zotero instance, and all object versions are local to that instance, with no relation to Web API versions. See [Server ID](#server_id) and [Object Versions](#object_versions).
-   Only data for the locally logged-in user is available. Pass `0` as the user ID or the user's actual numeric ID, which can be found on the [API Keys](/settings/keys) page. Requests for any other user ID return `400`.
-   Group metadata is limited to what's needed to identify the group; permissions, member lists, and similar details are not included. Group metadata is also read-only, since it's maintained by the server.
-   Atom is not supported. Requests with `format=atom` or `content=atom` return `501 Not Implemented`.
-   Item type and field endpoints (see [Item Types and Fields](dev/web_api/v3/types_and_fields)) return localized names in the user's locale. The `locale` query parameter is ignored. `/api/creatorFields` is the exception and always returns English names, matching the Web API.
-   Results are not paginated by default. The local API will return the full set of matching objects in one response, since nothing has to be transferred over the network. The `limit` and `start` parameters still work if you want them, and `Link` headers are still included.
-   Partial (binary diff) file uploads are not supported. `PATCH <userOrGroupPrefix>/items/<itemKey>/file` will fail with `405 Method Not Allowed`. Upload the full file instead.
-   The implementation aims to match the Web API's documented behavior but does not attempt to replicate every implementation detail. Sort order on equal keys, quicksearch matching, and the exact JSON produced for an object may differ in minor ways. Clients that depend on undefined behavior or unusual corner cases of the Web API should be tested against both implementations.

The local API also supports a few things the Web API does not:

-   `<userOrGroupPrefix>/searches/<searchKey>/items` returns the items matching a saved search. The Web API exposes search metadata but does not actually execute searches.
-   `<userOrGroupPrefix>/items/<itemKey>/file` returns a `302` redirect to a `file://` URL for the attachment on disk, and `/file/view` does the same. `/file/view/url` returns the URL as plain text rather than redirecting.
-   In Zotero 10+, `POST /api/local/authorize` requests permission to make changes.

Responses include a `Zotero-Schema-Version` header reflecting the schema version of the local Zotero instance, which may lag behind or run ahead of the version served by the Web API.

## Server ID

In Zotero 10+, every local API response includes a `Zotero-Server-ID` header containing a string that identifies the Zotero instance being talked to:

    Zotero-Server-ID: sPMHtLD6HHBd

The ID is stored in the Zotero database, so it follows the user's data rather than the installation, and it stays the same across restarts and upgrades. A different ID means a different database and therefore a different set of object versions and keys.

Clients should read the ID from any response — a bare `GET /api/` is enough — cache it, and pass it back in the `Zotero-Server-ID` request header:

-   On read requests, the header is optional. If provided, it must match the current server, or the request is rejected with `412 Precondition Failed`.
-   On write requests, including `POST /api/local/authorize`, the header is required. A write without it is rejected with `428 Precondition Required`.

A `412` means the client is talking to a Zotero instance other than the one its cached data came from. Discard cached data, including versions, and start over with the new ID.

Clients that store data between runs should partition it by server ID. The Web API does not currently provide or validate `Zotero-Server-ID`, but it will in the future, so treat Web API data as belonging to its own partition.

## Object Versions

In Zotero 10+, data object versions returned by the local API are maintained locally, not by the sync server. They're incremented once per library per transaction whenever an object in that library is saved or deleted, whether the change comes from the user, a sync, or a local API write.

Local versions in Zotero 10+ therefore have **no relation** to Web API versions, and no relation to the local versions reported by any other Zotero instance. This applies everywhere versions appear: the `version` property of object JSON, `Last-Modified-Version` response headers, `format=versions` responses, `?since=` filtering, and the `If-Unmodified-Since-Version` and per-object `version` preconditions used for writes.

Earlier versions of Zotero reported synced versions from the local API, which were `0` for objects that had never been synced and didn't change when an object was modified locally. Local versions are usually much lower than the synced versions those releases returned, so local API clients should discard any stored versions rather than compare them to new ones.

Group metadata is the exception: `<userOrGroupPrefix>/groups` and `/groups/<groupID>` report the synced group version, which has no local counterpart. Objects *within* group libraries — their items, collections, and searches — use local versions like all others.

## Authorizing Writes

Local API keys (Zotero 10+) are unrelated to zotero.org API keys and can't be created in advance. A client asks for one at runtime:

    POST /api/local/authorize
    Content-Type: application/json
    Zotero-Server-ID: <serverID>

    { "appName": "My Application" }

Zotero shows a dialog naming the application and asking the user to allow the change, with "Allow", "Always Allow", and "Deny" buttons. On approval, the response is

    { "key": "<32-character key>", "remember": false }

`remember` is `true` if the user chose "Always Allow", in which case the key can be reused indefinitely. Otherwise the key is single-use: the first write request that successfully validates it consumes it, and a subsequent write needs a new key. Clients should always be prepared to handle a `401` from a write request by authorizing again.

If the user denies the request, the response is `403 Forbidden` with `{ "denied": true }`. To limit prompt spam, no more than five requests that would show a dialog are accepted per minute; further requests return `429 Too Many Requests` with a `Retry-After` header.

Pass the key with each write request, the same way as a Web API key:

1.  As an HTTP header in the form `Zotero-API-Key: <key>` (recommended)
2.  As an HTTP header in the form `Authorization: Bearer <key>`
3.  As a URL query parameter, in the form `key=<key>` (not recommended)

A write request with no key or an unrecognized key returns `401 Unauthorized` with a `WWW-Authenticate: Zotero-API-Key realm="Zotero Local API"` header.

Keys are stored with the user's profile, and unlike Web API keys, they aren't scoped: a key allows writes to any library the user can edit. Users can revoke all remembered authorizations at any time with the "Clear Write Authorizations" button in Settings → Advanced.

## Write Requests

In Zotero 10+, `POST`, `PUT`, `PATCH`, and `DELETE` are supported for items, collections, and saved searches. The local API also supports tag deletion, full-text writes, and file uploads. Requests and responses follow the same format as the Web API, described in [Write Requests](dev/web_api/v3/write_requests).

`Zotero-Write-Token` is supported, but tokens are cached in memory, so they're forgotten when Zotero restarts.

Changes made through the local API are ordinary local changes. They're visible in the Zotero UI immediately and will be uploaded to zotero.org on the next sync if the library is synced.

## File Uploads

In Zotero 10+, the three-phase [file upload](dev/web_api/v3/file_upload) flow works locally, with the uploads going to Zotero itself rather than to S3:

1.  `POST <userOrGroupPrefix>/items/<itemKey>/file` with `md5`, `filename`, `filesize`, and `mtime` parameters, and an `If-Match` or `If-None-Match` header, exactly as in the Web API. The response contains a `url` pointing at `/api/local/uploads/<uploadKey>` on the local server, along with `uploadKey`, `contentType`, and empty `prefix` and `suffix` strings. If the file on disk already matches the given MD5, the response is `{ "exists": 1 }` and no upload is needed.
2.  `POST` the file contents to `url`. A successful upload returns `201 Created`. The received bytes must hash to the `md5` provided in the previous step, or the response is `400`. This request doesn't need `Zotero-Server-ID` or an API key, since the upload key authorizes it. Upload keys expire after an hour.
3.  `POST` to the file endpoint again with `upload=<uploadKey>`, repeating the `If-Match` or `If-None-Match` header. Zotero moves the uploaded file into the attachment's storage directory and updates the item, returning `204 No Content` with the new library version in `Last-Modified-Version`.

Uploads are only accepted for stored-file attachments (`imported_file` and `imported_url`); other attachment types return `400`. Files must be under 4 GB.

## Full-Text Content

`PUT <userOrGroupPrefix>/items/<itemKey>/fulltext` sets an attachment's full-text content as documented in [Full-Text Content](dev/web_api/v3/fulltext_content), and `POST <userOrGroupPrefix>/fulltext` accepts an array of up to 10 entries, each with a `key` property, returning the same result object as other multi-object writes. Bulk writes require `If-Unmodified-Since-Version`.

The versions returned by `GET <userOrGroupPrefix>/fulltext?since=<version>` and in `Last-Modified-Version` are local versions, so partitioning is essential. Items whose content type Zotero doesn't index return `400`.

See [Basics](dev/web_api/v3/basics) for an overview of the API as a whole.

# Zotero Web API Item Type/Field Requests

For a [Zotero Web API](dev/web_api/v3/) client to present an editing UI to its users, it must know what combinations of Zotero item types, fields, and creator types are valid. Clients can request this data from the Zotero API.

As schema changes are currently rare, clients should cache type/field data for a period of time (e.g., one hour) without making further requests. Subsequent requests for new data should then include `If-Modified-Since` headers containing the contents of the `Last-Modified` header from the original response. If no changes have occurred, the server will return a `304 Not Modified` and clients should continue to use cached data for the same period of time. *[Conditional requests -- i.e. `If-Modified-Since` -- are not yet implemented.]*

User-friendly type/field names will be returned in English by default. Clients can request names in other languages by passing a `locale` parameter (e.g., `GET /itemTypes?locale=fr-FR`).

Note: the entire schema, including translations for all locales, can also be downloaded as a single file at <https://api.zotero.org/schema>. See the [GitHub repository of the schema](https://github.com/zotero/zotero-schema) for caching instructions.

### Getting All Item Types

    GET /itemTypes
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    [
      { "itemType" : "book", "localized" : "Book" },
      { "itemType" : "note", "localized" : "Note" },
      (…)
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | Locale not supported. |

### Getting All Item Fields

    GET /itemFields
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    [
      { "field" : "title", "localized" : "Title" },
      { "field" : "url", "localized" : "URL" },
      (…)
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | Locale not supported. |

### Getting All Valid Fields for an Item Type

Note: API consumers intending to write to the server should generally use [/items/new](#getting_a_template_for_a_new_item) combined with [/itemTypes](#getting_all_item_types) instead of this request.

    GET /itemTypeFields?itemType=book
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    [
      { "field" : "title", "localized" : "Title" },
      { "field" : "abstractNote", "localized" : "Abstract" },
      (…)
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | Locale not supported. |

### Getting Valid Creator Types for an Item Type

    GET /itemTypeCreatorTypes?itemType=book

    [
      { "creatorType" : "author", "localized" : "Author" },
      { "creatorType" : "editor", "localized" : "Editor" },
      (…)
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | 'itemType' is unspecified or invalid; locale not supported. |

### Getting Localized Creator Fields

    GET /creatorFields
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    [
      { "field" : "firstName", "localized" : "First" },
      { "field" : "lastName", "localized" : "Last" },
      { "field" : "name", "localized" : "Name" }
    ]

| Common responses | |
| :--- | :--- |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | Locale not supported. |

### Getting a Template for a New Item

    GET /items/new?itemType=book
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    {
      "itemType" : "book",
      "title" : "",
      "creators" : [
        {
          "creatorType" : "author",
          "firstName" : "",
          "lastName" : ""
        }
      ],
      "url" : "",
      (...),
      "tags" : [],
      "collections" : [],
      "relations" : {}
    }

    GET /items/new?itemType=note
    If-Modified-Since: Mon, 14 Mar 2011 22:30:17 GMT

    {
      "itemType" : "note",
      "note" : "",
      "tags" : [],
      "collections" : [],
      "relations" : {}
    }

TODO: attachment creation (see [File Uploads](dev/web_api/v3/file_upload))

| Common responses | |
| :--- | :--- |
| `200 OK` | |
| `304 Not Modified` | No changes have occurred since `If-Modified-Since` time. |
| `400 Bad Request` | `itemType` is unspecified or invalid. |

# Zotero Web API Write Requests

This page documents the write methods of the [Zotero Web API](dev/web_api/v3/). See the [Basics](dev/web_api/v3/basics) page for basic information on accessing the API, including possible HTTP status codes not listed here.

An [API key](dev/web_api/v3/basics#authentication) with write access to a given library is necessary to use write methods.

In Zotero 10+, the [local API](dev/web_api/v3/local_api) supports the same write methods against the local database. Writes there use a local API key granted by the user at runtime and check preconditions against local object versions; see [Local API](dev/web_api/v3/local_api#write_requests) for the differences.

## JSON Object Data

By default, objects returned from the API in `format=json` mode include a `data` property containing "editable JSON" — that is, all the object fields that can be modified and sent back to the server:

    {
      "key": "ABCD2345",
      "version": 1,
      "library": { ... },
      "links": { ... },
      "meta": { ... },
      "data": {
        "key": "ABCD2345",
        "version": 1,
        "itemType": "webpage",
        "title": "Zotero Quick Start Guide",
        "creators": [
            {
                "creatorType": "author",
                "name": "Center for History and New Media"
            }
        ],
        "abstractNote": "",
        "websiteTitle": "Zotero",
        "websiteType": "",
        "date": "",
        "shortTitle": "",
        "url": "https://www.zotero.org/support/quick_start_guide",
        "accessDate": "2014-06-12T21:28:55Z",
        "language": "",
        "rights": "",
        "extra": "",
        "dateAdded": "2014-06-12T21:28:55Z",
        "dateModified": "2014-06-12T21:28:55Z",
        "tags": [],
        "collections": [],
        "relations": {}
      }
    }

There are two ways to make changes to the provided JSON:

### Programmatic Approach

For programmatic access to the API, the recommended approach is to extract the editable JSON from the `data` property, make changes as necesssary, and upload just the editable JSON back to the API. For new items, an [empty template](dev/web_api/v3/types_and_fields#getting_a_template_for_a_new_item) of the editable JSON can be retrieved from the API.

This approach reduces upload bandwidth by sending only the data that is actually processed by the server. (For an even more efficient upload, the HTTP `PATCH` method, discussed below, can be used to send only the fields that have changed.) The examples in this documentation assume programmatic access.

### REST Approach

For more casual access, the API supports standard REST behavior, allowing the entire downloaded JSON to be reuploaded. This allows edits to be performed without writing a single line of code:

    $ URL="https://api.zotero.org/users/1234567/items"
    $ API_KEY="P9NiFoyLeZu2bZNvvuQPDWsd"
    $ curl -H "Zotero-API-Key: $API_KEY" $URL > items.json
    $ vi items.json  # edit the item data
    $ curl -H "Zotero-API-Key: $API_KEY" -d @items.json -v $URL

In this example, a JSON array of items is being saved to a text file, modified in a text editor, and then POSTed back to the same URL.

This approach allows a complicated task such as batch editing to be performed using only cURL and a text editor. Any objects modified in the text file will be updated on the server, while unmodified objects will be left unchanged.

A similar process can be used with PUT for individual objects:

    $ URL="https://api.zotero.org/users/1234567/items/ABCD2345"
    $ API_KEY="P9NiFoyLeZu2bZNvvuQPDWsd"
    $ curl -H "Zotero-API-Key: $API_KEY" $URL > item.json
    $ vi items.json  # edit the item data
    $ curl -H "Zotero-API-Key: $API_KEY" -X PUT -d @item.json -v $URL

Note that when uploading full JSON, only the `data` property is processed. All other properties (`library`, `links`, `meta`, etc.) are ignored.

## Item Requests

### Creating an Item

When creating a new item, first get empty JSON for the item type with an [item template request](dev/web_api/v3/types_and_fields#getting_a_template_for_a_new_item) (or use a cached version of the template). Then modify it and resubmit it to the server in an array:

    POST <userOrGroupPrefix>/items
    Content-Type: application/json
    Zotero-Write-Token: <write token> or If-Unmodified-Since-Version: <last library version>

    [
      {
        "itemType" : "book",
        "title" : "My Book",
        "creators" : [
          {
            "creatorType":"author",
            "firstName" : "Sam",
            "lastName" : "McAuthor"
          },
          {
            "creatorType":"editor",
            "name" : "John T. Singlefield"
          }
        ],
        "tags" : [
          { "tag" : "awesome" },
          { "tag" : "rad", "type" : 1 }
        ],
        "collections" : [
          "BCDE3456", "CDEF4567"
        ],
        "relations" : {
          "owl:sameAs" : "http://zotero.org/groups/1/items/JKLM6543",
          "dc:relation" : "http://zotero.org/groups/1/items/PQRS6789",
          "dc:replaces" : "http://zotero.org/users/1/items/BCDE5432"
        }
      }
    ]

All properties other than `itemType`, `tags`, `collections`, and `relations` are optional.

| Common responses | |
| :--- | :--- |
| `200 OK` | The request completed. See the response JSON for status of individual writes. |
| `400 Bad Request` | Invalid type/field; unparseable JSON |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The version provided in `If-Unmodified-Since-Version` is out of date, or the provided `Zotero-Write-Token` has already been submitted. |
| `413 Request Entity Too Large` | Too many items submitted |

`200 OK` response:

    {
      "success": {
        "0": "<itemKey>"
      },
      "unchanged": {},
      "failed": {},
      }
    }

See [Creating Multiple Objects](#creating_multiple_objects) for more information on the response format.

### Creating Multiple Items

See [Creating Multiple Objects](#creating_multiple_objects).

### Updating an Existing Item

To update an existing item, first retrieve the current version of the item:

    GET <userOrGroupPrefix>/items/<itemKey>

The editable data, similar to the item data shown above in [Creating an Item](#creating_an_item), will be found in the `data` property in the response.

The API supports two ways of modifying item data: by uploading full item data (`PUT`) or by sending just the data that changed (`PATCH`).

#### Full-item updating (PUT)

With `PUT`, you submit the item's complete editable JSON to the server, typically by modifying the downloaded editable JSON — that is, the contents of the `data` property — directly and resubmitting it:

    PUT <userOrGroupPrefix>/items/<itemKey>
    Content-Type: application/json

    {
      "key": "ABCD2345",
      "version": 1,
      "itemType" : "book",
      "title" : "My Amazing Book",
      "creators" : [
        {
          "creatorType":"author",
          "firstName" : "Sam",
          "lastName" : "McAuthor"
        },
        {
          "creatorType":"editor",
          "name" : "Jenny L. Singlefield"
        }
      ],
      "tags" : [
        { "tag" : "awesome" },
        { "tag" : "rad", "type" : 1 }
      ],
      "collections" : [
        "BCDE3456", "CDEF4567"
      ],
      "relations" : {
        "owl:sameAs" : "http://zotero.org/groups/1/items/JKLM6543",
        "dc:relation" : "http://zotero.org/groups/1/items/PQRS6789",
        "dc:replaces" : "http://zotero.org/users/1/items/BCDE5432"
      }
    }

All properties other than `itemType`, `tags`, `collections`, and `relations` are optional. Any existing fields not specified will be removed from the item. If `creators`, `tags`, `collections`, or `relations` are empty, any associated creators/tags/collections/relations will be removed from the item.

#### Partial-item updating (PATCH)

With `PATCH`, you can submit just the properties that have actually changed, for a potentially much more efficient operation. Properties not included in the uploaded JSON are left untouched on the server. To clear a property, pass an empty string or an empty array as appropriate.

    PATCH <userOrGroupPrefix>/items/<itemKey>
    If-Unmodified-Since-Version: <last item version>

    {
      "date" : "2013"
      "collections" : [
        "BCDE3456", "CDEF4567"
      ]
    }

This would add a `date` field to the item and add it in the two specified collections if not already present. Array properties are interpreted as complete lists, so omitting a collection key would cause the item to be removed from that collection.

The `PATCH` behavior is also available when [modifying multiple items](dev/web_api/v3/write_requests#updating_multiple_objects) via `POST`.

#### Both PUT and PATCH

Notes and attachments can be made child items by assigning the parent item's key to the `parentItem` property. If parent and child items are created in the same `POST` request, the child items must appear after the parent item in the array of items, with a locally created [item key](#object_keys).

The item's current version number is included in the `version` JSON property, as well as in the `Last-Modified-Version` header of single-item requests. `PUT` and `PATCH` requests must include the item's current version number in either the `version` property or the `If-Unmodified-Since-Version` header. (`version` is included in responses from the API, so clients that simply modify the editable data do not need to bother with a version header.) If the item has been changed on the server since the item was retrieved, the write request will be rejected with a `412 Precondition Failed` error, and the most recent version of the item will have to be retrieved from the server before changes can be made. See [Version Numbers](dev/web_api/v3/syncing#version_numbers) for more on library and object versioning.

| Common responses | |
| :--- | :--- |
| `204 No Content` | The item was successfully updated. |
| `400 Bad Request` | Invalid type/field; unparseable JSON |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The item has changed since retrieval (i.e., the provided item version no longer matches). |

### Updating Multiple Items

See [Updating Multiple Objects](#updating_multiple_objects).

### Deleting an Item

    DELETE <userOrGroupPrefix>/items/<itemKey>
    If-Unmodified-Since-Version: <last item version>

| Common responses | |
| :--- | :--- |
| `204 No Content` | The item was deleted. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The item has changed since retrieval (i.e., the provided item version no longer matches). |
| `428 Precondition Required` | `If-Unmodified-Since-Version` was not provided. |

### Deleting Multiple Items

Up to 50 items can be deleted in a single request.

    DELETE <userOrGroupPrefix>/items?itemKey=<key>,<key>,<key>
    If-Unmodified-Since-Version: <last library version>

    204 No Content
    Last-Modified-Version: <library version>

| Common responses | |
| :--- | :--- |
| `204 No Content` | The items were deleted. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The library has changed since the specified version. |
| `428 Precondition Required` | `If-Unmodified-Since-Version` was not provided. |

## Collection Requests

### Creating a Collection

    POST <userOrGroupPrefix>/collections
    Content-Type: application/json
    Zotero-Write-Token: <write token> or If-Unmodified-Since-Version: <last library version>

    [
      {
        "name" : "My Collection",
        "parentCollection" : "QRST9876"
      }
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | The request completed without a general error. See the response JSON for status of individual writes. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The version provided in `If-Unmodified-Since-Version` is out of date, or the provided `Zotero-Write-Token` has already been submitted. |

### Updating an Existing Collection

    GET <userOrGroupPrefix>/collections/<collectionKey>

Editable JSON will be found in the `data` property.

    PUT <userOrGroupPrefix>/collections/<collectionKey>
    Content-Type: application/json

    {
      "key" : "DM2F65CA",
      "version" : 156,
      "name" : "My Collection",
      "parentCollection" : false
    }

| Common responses | |
| :--- | :--- |
| `200 OK` | The collection was updated. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The collection has changed since retrieval (i.e., the provided collection version no longer matches). |

### Collection-Item Membership

Items can be added to or removed from collections via the `collections` property in the item JSON.

### Deleting a Collection

    DELETE <userOrGroupPrefix>/collections/<collectionKey>
    If-Unmodified-Since-Version: <last collection version>

| Common responses | |
| :--- | :--- |
| `204 No Content` | The collection was deleted. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The collection has changed since retrieval (i.e., the provided ETag no longer matches). |

### Deleting Multiple Collections

Up to 50 collections can be deleted in a single request.

    DELETE <userOrGroupPrefix>/collections?collectionKey=<collectionKey>,<collectionKey>,<collectionKey>
    If-Unmodified-Since-Version: <last library version>

    204 No Content
    Last-Modified-Version: <library version>

| Common responses | |
| :--- | :--- |
| `204 No Content` | The collections were deleted. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The library has changed since the specified version. |

## Saved Search Requests

### Creating a Search

    POST <userOrGroupPrefix>/searches
    Content-Type: application/json
    Zotero-Write-Token: <write token> or If-Unmodified-Since-Version: <last library version>

    [
      {
        "name": "My Search",
        "conditions": [
          {
            "condition": "title",
            "operator": "contains",
            "value": "foo"
          },
          {
            "condition": "date",
            "operator": "isInTheLast",
            "value": "7 days"
          }
        ]
      }
    ]

| Common responses | |
| :--- | :--- |
| `200 OK` | The request completed without a general error. See the response JSON for status of individual writes. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The version provided in `If-Unmodified-Since-Version` is out of date, or the provided `Zotero-Write-Token` has already been submitted. |

### Deleting Multiple Searches

Up to 50 searches can be deleted in a single request.

    DELETE <userOrGroupPrefix>/searches?searchKey=<searchKey>,<searchKey>,<searchKey>
    If-Unmodified-Since-Version: <last library version>

    204 No Content
    Last-Modified-Version: <library version>

| Common responses | |
| :--- | :--- |
| `204 No Content` | The searches were deleted. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The library has changed since the specified version. |

## Tag Requests

### Deleting Multiple Tags

Up to 50 tags can be deleted in a single request.

    DELETE <userOrGroupPrefix>/tags?tag=<URL-encoded tag 1> || <URL-encoded tag 2> || <URL-encoded tag 3>
    If-Unmodified-Since-Version: <last library version>

    204 No Content
    Last-Modified-Version: <library version>

## Multi-Object Requests

### Creating Multiple Objects

Up to 50 collections, saved searches, or items can be created in a single request by including multiple objects in an array:

    POST <userOrGroupPrefix>/collections
    Content-Type: application/json
    Zotero-Write-Token: <write token> or If-Unmodified-Since-Version: <last library version>

    [
      {
        "name" : "My Collection",
        "parentCollection": "QRST9876"
      },
      {
        "name": "My Other Collection"
      }
    ]

For [syncing](dev/web_api/v3/syncing) objects with predetermined keys, an [object key](#object_keys) can also be provided with new objects.

`200` response:

    Content-Type: application/json
    Last-Modified-Version: <library version>

    {
      "successful": {
        "0": "<saved object>",
        "2": "<saved object>"
      },
      "unchanged": {
        "4": "<objectKey>"
      }
      "failed": {
        "1": {
          "key": "<objectKey>",
          "code": <HTTP response code>,
          "message": "<error message>"
        },
        "3": {
          "key": "<objectKey>",
          "code": <HTTP response code>,
          "message": "<error message>"
        },
      }
    }

The keys of the `successful`, `unchanged`, and `failed` objects are the numeric indexes of the Zotero objects in the uploaded array. The `Last-Modified-Version` is the version that has been assigned to any Zotero objects in the `successful` object — that is, objects that were modified in this request.

| Common responses | |
| :--- | :--- |
| `200 OK` | The objects were uploaded. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The version provided in `If-Unmodified-Since-Version` is out of date, or the provided `Zotero-Write-Token` has already been submitted. |

#### Updating Multiple Objects

Up to 50 collections, saved searches, or items can be updated in a single request.

Follow the instructions in [Creating Multiple Objects](#creating_multiple_objects), but include a `key` and `version` property in each object. If modifying editable JSON returned from the API, these properties will already exist and shouldn't be modified. As an alternative to individual `version` properties, the last-known library version can be passed via the `If-Unmodified-Since-Version` HTTP Header.

Items can also include `dateAdded` and `dateModified` properties containing timestamps in [ISO 8601 format](http://en.wikipedia.org/wiki/ISO_8601) (e.g., "2014-06-10T13:52:43Z"). If `dateAdded` is included with an existing item, it must match the existing `dateAdded` value or else the API will return a 400 error. If a new `dateModified` time is not included with an update to existing item, the item's `dateModified` value will be set to the current time. Editable JSON returned from the API includes `dateAdded` and `dateModified` in the correct format, so clients that are content with server-set modification times can simply ignore these properties.

    POST <userOrGroupPrefix>/collections
    Content-Type: application/json

    [
      {
        "key": "BD85JEM4",
        "version": 414,
        "name": "My Collection",
        "parentCollection": false
      },
      {
        "key": "MNC5SAPD",
        "version": 416
        "name": "My Subcollection",
        "parentCollection": "BD85JEM4"
      }
    ]

The response is the same as that in [Creating Multiple Objects](#creating_multiple_objects).

Note that `POST` follows `PATCH` semantics, meaning that any properties not specified will be left untouched on the server. To erase an existing property, include it with an empty string or `false` as the value.

## Object Keys

While the server will automatically generate valid keys for uploaded objects, in some situations, such as when [syncing](dev/web_api/v3/syncing) or creating a parent and child item in the same request, it may be desirable or necessary to create object keys locally.

Local object keys should conform to the pattern `/[23456789ABCDEFGHIJKLMNPQRSTUVWXYZ]{8}/`.

## Zotero-Write-Token

    Zotero-Write-Token: 19a4f01ad623aa7214f82347e3711f56

`Zotero-Write-Token` is an optional HTTP header, containing a client-generated random 32-character identifier string, that can be included with unversioned write requests to prevent them from being processed more than once (e.g., if a user clicks a form submit button twice). The Zotero server caches write tokens for successful requests for 12 hours, and subsequent requests from the same API key using the same write token will be rejected with a `412 Precondition Failed` status code. If a request fails, the write token will not be stored.

If using [versioned write requests](dev/web_api/v3/syncing#version_numbers) (i.e., those that include an `If-Unmodified-Since-Version` HTTP header or individual object `version` properties), `Zotero-Write-Token` is redundant and should be omitted.

In Zotero 10+, the [local API](dev/web_api/v3/local_api) also caches write tokens for 12 hours, but only in memory, so a token is forgotten if Zotero restarts.

## Examples

See the [Syncing](dev/web_api/v3/syncing) page for an example workflow that puts together read and write methods for complete and efficient syncing of Zotero data via the API.

# Zotero Web API File Uploads

In addition to providing ways to [read](dev/web_api/v3/basics#read_requests) and [write](dev/web_api/v3/write_requests) online library data, the Zotero Web API allows you to upload attachment files.

The exact process depends on whether you are adding a new attachment file or modifying an existing one and whether you are performing a full upload or uploading a binary diff.

In Zotero 10+, the [local API](dev/web_api/v3/local_api#file_uploads) supports the same full-upload flow, with the file going to Zotero rather than to S3. Binary diffs are not supported locally.

## 1a) Create a new attachment

### i. Get attachment item template

    GET /items/new?itemType=attachment&linkMode={imported_file,imported_url,linked_file,linked_url}

    {
      "itemType": "attachment",
      "linkMode": "imported_url",
      "title": "",
      "accessDate": "",
      "url": "",
      "note": "",
      "tags": [],
      "relations": {},
      "contentType": "",
      "charset": "",
      "filename": "",
      "md5": null,
      "mtime": null
    }

### ii. Create child attachment item

    POST /users/<userID>/items
    Content-Type: application/json
    Zotero-Write-Token: <token>

    [
      {
        "itemType": "attachment",
        "parentItem": "ABCD2345",
        "linkMode": "imported_url",
        "title": "My Document",
        "accessDate": "2012-03-14T17:45:54Z",
        "url": "http://example.com/doc.pdf",
        "note": "",
        "tags": [],
        "relations": {},
        "contentType": "application/pdf",
        "charset": "",
        "filename": "doc.pdf",
        "md5": null,
        "mtime": null
      }
    ]

`md5` and `mtime` can be edited directly in personal libraries for WebDAV-based file syncing. They should not be edited directly when using Zotero File Storage, which provides an atomic method (detailed below) for setting the properties along with the corresponding file.

Top-level attachments can be created by excluding the `parentItem` property or setting it to `false`. Though the API allows all attachments to be made top-level items for backward-compatibility, it is recommended that only file attachments (`imported_file`/`linked_file`) and PDF imported web attachments (`imported_url` with content type `application/pdf`) be allowed as top-level items, as in the Zotero client.

## 1b) Modify an existing attachment

### i. Retrieve the attachment information

    GET /users/<userID>/items/<itemKey>

    {
      "key": "ABCD2345",
      "version": 124,
      "library": { ... },
      ...
      "data": {
        "key": "ABCD2345",
        "version": 124,
        "itemType": "attachment",
        "linkMode": "imported_file",
        "title": "My Document",
        "note": "",
        "tags": [],
        "relations": {},
        "contentType": "text/plain",
        "charset": "utf-8",
        "filename": "doc.txt",
        "md5": "4fa38e3f2c360ca181e633d02bab91f5",
        "mtime": "1331171741767"
      }
    }

### ii. Download the existing file

    GET /users/<userID>/items/<itemKey>/file

Check the `ETag` header of the response to make sure it matches the attachment item's `md5` value. If it doesn't, check the attachment item again. If the attachment item still has a different hash, the latest version of the file may be available only via WebDAV, not via Zotero File Storage, and it is up to the client how to proceed.

Save the file as `filename` and set the modification time to the `mtime` provided in the attachment item.

### iii. Make changes to the file

Note that to perform a faster partial upload using a binary diff, you must save a copy of the file before changes are made.

## 2) Get upload authorization

    POST /users/<userID>/items/<itemKey>/file
    Content-Type: application/x-www-form-urlencoded
    If-None-Match: *

    md5=<hash>&filename=<filename>&filesize=<bytes>&mtime=<milliseconds>

For existing attachments, use `If-Match: <hash>` in place of `If-None-Match: *`, where <hash> is the previous MD5 hash of the file (as provided in the `ETag` header when downloading it).

Note that `mtime` must be provided in milliseconds, not seconds.

A successful `200` response returns one of two possible JSON objects:

    {
      "url": ...,
      "contentType": ...,
      "prefix": ...,
      "suffix": ...,
      "uploadKey": ...
    }

or

    { "exists": 1 }

In the latter case, the file already exists on the server and was successfully associated with the specified item. No further action is necessary.

| Common responses | |
| :--- | :--- |
| `200 OK` | The upload was authorized or the file already exists. |
| `403 Forbidden` | File editing is denied. |
| `409 Conflict` | The target library is locked. |
| `412 Precondition Failed` | The file has changed remotely since retrieval (i.e., the provided ETag no longer matches). Conflict resolution is left to the client. |
| `413 Request Entity Too Large` | The upload would exceed the storage quota of the library owner. |
| `428 Precondition Required` | If-Match or If-None-Match was not provided. |
| `429 Too Many Requests` | Too many unfinished uploads. Try again after the number of seconds specified in the `Retry-After` header. |

## 3a) Full upload

### i. POST file

Concatenate `prefix`, the file contents, and `suffix` and POST to `url` with the `Content-Type` header set to `contentType`.

`prefix` and `suffix` are strings containing multipart/form-data. In some environments, it may be easier to work directly with the form parameters. Add `params=1` to the upload authorization request above to retrieve the individual parameters in a `params` array, which will replace `contentType`, `prefix`, and `suffix`.

| Common responses | |
| :--- | :--- |
| `201 Created` | The file was successfully uploaded. |

### ii. Register upload

    POST /users/<userID>/items/<itemKey>/file
    Content-Type: application/x-www-form-urlencoded
    If-None-Match: *

    upload=<uploadKey>

For existing attachments, use `If-Match: <hash>`, where <hash> is the previous MD5 hash of the file, provided as the `md5` property in the attachment item.

| Common responses | |
| :--- | :--- |
| `204 No Content` | The upload was successfully registered. |
| `412 Precondition Failed` | The file has changed remotely since retrieval (i.e., the provided ETag no longer matches). |

After the upload has been registered, the attachment item will reflect the new metadata (`filename`, `mtime`, `md5`). The `Zotero-Library-Version` header of the response will reflect the new version of the item.

## 3b) Partial upload

    PATCH /users/<userID>/items/<itemKey>/file?algorithm={xdelta,vcdiff,bsdiff}&upload=<uploadKey>
    If-Match: <previous-value-of-md5-property>

    <Binary diff of old and new versions>

For best results, we recommend using Xdelta version 3 with the "`-9 -S djw`" flags. bsdiff takes significantly longer to generate diffs. 'vcdiff' is an alias for 'xdelta', as Xdelta3 can process diffs in VCDIFF format.

Clients may wish to automatically fall back to a full upload — possibly with some form of warning — if HTTP PATCH is not supported by a user's proxy server (indicated, in theory, by a `405 Method Not Allowed`).

After the upload has finished, the attachment item will reflect the new metadata. The `Zotero-Library-Version` header of the response will reflect the new version of the item.

| Common responses | |
| :--- | :--- |
| `204 No Content` | The patch was successfully applied. |
| `409 Conflict` | The target library is locked; the patched file does not match the provided MD5 hash or file size |
| `428 Precondition Required` | If-Match or If-None-Match was not provided. |
| `429 Too Many Requests` | Too many unfinished uploads. Try again after the number of seconds specified in the `Retry-After` header. |

# Zotero Web API Full-Text Content Requests

This page documents the methods to access full-text content of Zotero items via the [Zotero Web API](dev/web_api/v3/). See the [Basics](dev/web_api/v3/basics) page for basic information on accessing the API, including possible HTTP status codes not listed here.

These methods are also available in the [local API](dev/web_api/v3/local_api#full-text_content). In Zotero 10+, the versions returned are local versions rather than server versions.

### Getting new full-text content

    GET <userOrGroupPrefix>/fulltext?since=<version>

    Content-Type: application/json
    Last-Modified-Version: <library version>

    {
        "<itemKey>": <version>,
        "<itemKey>": <version>,
        "<itemKey>": <version>
    }

For each item with a full-text content version greater than stored locally, get the item's full-text content, as described below.

| Common responses | |
| :--- | :--- |
| `200 OK` | Full-text content was successfully retrieved. |
| `400 Bad Request` | The 'since' parameter was not provided. |

### Getting an item's full-text content

    GET <userOrGroupPrefix>/items/<itemKey>/fulltext

`<itemKey>` should correspond to an existing attachment item.

    Content-Type: application/json
    Last-Modified-Version: <version of item's full-text content>

    {
        "content": "This is full-text content.",
        "indexedPages": 50,
        "totalPages": 50
    }

`indexedChars` and `totalChars` are used for text documents, while `indexedPages` and `totalPages` are used for PDFs.

| Common responses | |
| :--- | :--- |
| `200 OK` | Full-text content was found for the given item. |
| `404 Not Found` | The item wasn't found, or no full-text content was found for the given item. |

### Setting an item's full-text content

    PUT <userOrGroupPrefix>/items/<itemKey>/fulltext
    Content-Type: application/json

    {
        "content": "This is full-text content.",
        "indexedChars": 26,
        "totalChars": 26
    }

`<itemKey>` should correspond to an existing attachment item.

For text documents, include `indexedChars` and `totalChars`. For PDFs, include `indexedPages` and `totalPages`.

| Common responses | |
| :--- | :--- |
| `204 No Content` | The item's full-text content was updated. |
| `400 Bad Request` | Invalid JSON was provided. |
| `404 Not Found` | The item wasn't found or was not an attachment. |

### Searching for items by full-text content

See the `q` and `qmode` [search parameters](dev/web_api/v3/basics#search_parameters).

# Zotero Web API Syncing

This document outlines the recommended steps for synchronizing a [Zotero Web API](dev/web_api/v3/) client with the Zotero server. Be sure you've read the [write request](dev/web_api/v3/write_requests) documentation for basic information on modifying data via the API.

TODO:

-   Incorporate [WebSocket handling](dev/web_api/v3/streaming_api)

## Sync Properties

In addition to standard object metadata (item field values, group names, etc.), clients should store the following properties:

-   A version number for metadata for each group
-   A version number for each library
-   A version number and a boolean `synced` flag for each syncable object
-   A list of downloaded objects that could not be processed and should be requested explicitly regardless of their remote version number (optional; see [Handling save errors](#Handling save errors) for details)

## Version Numbers

Every Zotero library and object (collection, item, etc.) on the server has an associated version number. The version number can be used to determine whether a client has up-to-date data for a library or object, allowing for efficient and safe syncing.

The API supports three custom HTTP headers that expose the versions: the `Last-Modified-Version` response header and the `If-Unmodified-Since-Version` and `If-Modified-Since-Version` request headers. The version number that the headers apply to depends on the request being made: for multiple-object requests such as `<userOrGroupPrefix>/items`, the headers apply to the entire library, whereas for single-object requests such as `<userOrGroupPrefix>/items/<itemKey>`, the headers apply to the individual object.

The version numbers are also accessible in several other ways, discussed below.

The version number is guaranteed to be monotonically increasing but is not guaranteed to increase sequentially, and clients should treat it as an opaque integer value.

#### Last-Modified-Version

The `Last-Modified-Version` response header indicates the current version of either a library (for multi-object requests) or an individual object (for single-object requests). If changes are made to a library in a write request, the library's version number will be increased, any objects modified in the same request will be set to the new version number, and the new version number will be returned in the `Last-Modified-Version` header.

#### If-Modified-Since-Version

The `If-Modified-Since-Version` request header can be used to efficiently check for new data. If `If-Modified-Since-Version: <libraryVersion>` is passed with a multi-object read request and data has not changed in the library since the specified version, the API will return `304 Not Modified`. If `If-Modified-Since-Version: <objectVersion>` is passed with a single-object read request, a `304 Not Modified` will be returned if the individual object has not changed.

#### If-Unmodified-Since-Version

The `If-Unmodified-Since-Version` request header is used to ensure that existing data won't be overwritten by a client with out-of-date data. All write requests that modify existing objects must include either the `If-Unmodified-Since-Version: <version>` header or a [#JSON version property](#JSON version property) for each object. If both are omitted, the API will return a `428 Precondition Required`.

For write requests to multi-object endpoints such as `<userOrGroupPrefix>/items`, the API will return `412 Precondition Failed` if the library has been modified since the passed version. For write requests to single-object endpoints such as `<userOrGroupPrefix>/items/<itemKey>`, the API will return a `412` if the object has been modified since the passed version.

Clients should generally use `If-Unmodified-Since-Version` for multi-object requests only if they have downloaded all server data for the object type being written. Otherwise, a client creating a new object could assign an object key that already exists on the server and accidentally overwrite the existing object.

`If-Unmodified-Since-Version` also enables more efficient syncs. Rather than first polling for remote updates, clients that have changes to upload should start by simply trying to perform the necessary [write requests](#iv_upload_modified_data), passing the current local library version in the `If-Unmodified-Since-Version` header. If updated data is available, the API will return `412 Precondition Failed`, indicating that the client must first retrieve the updated data. In the absence of a `412` for a write request, clients with local modifications do not need to check for remote changes explicitly.

`If-Unmodified-Since-Version: <version>` replaces the `If-Match: <etag>` header previously required for single-object writes.

#### JSON version property

`format=json` responses will include a `version` property in each object's editable JSON (the `data` property) indicating the current version of that object. This value will be identical to the `version` property supplied at the top level of the JSON object. For single-object requests, this will also be identical to the value of the `Last-Modified-Version` response header.

If included in JSON submitted back to the API, the JSON version property will behave equivalently to a single-object `If-Unmodified-Since-Version`: if the object has been modified since the specified version, the API will return a `412 Precondition Failed`. When writing objects that include object keys, either the request must include `If-Unmodified-Since-Version` or each object must include the JSON version property. When writing new objects with an object key in a request without `If-Unmodified-Since-Version`, use the special version 0 to indicate that the objects should not yet exist on the server.

While `If-Unmodified-Since-Version` and the JSON version property are not mutually exclusive for write requests, they are redundant, and generally clients should use one or the other depending on their interaction mechanism. See [#Partial-Library Syncing](#Partial-Library Syncing) for a discussion of possible syncing methods.

#### ?since=&lt;version&gt;

The `since` query parameter can be used to retrieve only objects modified since a specific version.

#### ?format=versions

`format=versions` is similar to `format=keys`, but instead of returning a newline-delimited list of object keys, it returns a JSON object with object versions keyed by object keys:

    {
      "<itemKey>": <version>,
      "<itemKey>": <version>,
      "<itemKey>": <version>
    }

Like `format=keys`, `format=versions` is not limited by a maximum number of results and returns all matching objects by default.

#### Local object versions

The use of local object versions during syncing, and the process for updating them, is described below.

When objects are created or modified locally by the user during regular usage, set `synced = false` to indicate that the object needs to be uploaded on the next sync. Give new objects version 0. Do not change the version when objects are modified outside of the sync process.

## Full-Library Syncing

The following steps are for complete syncing of Zotero libraries, such as to enable full offline usage. For tips on alternative syncing methods, see [Partial-Library Syncing](#partial_library_syncing).

### 1) Verify key access

    GET /keys/current

`200` Response:

    {
      {
        "userID": 12345
        "username": "Z User"
        "access": {
            "user": {
                "library": true
                "files": true
                "notes": true
                "write": true
            }
            "groups": {
                "all": {
                    "library": true
                    "write": true
                }
            }
        }
    }
    }

`/keys/current` returns information on the API key provided in the `Zotero-API-Key` header. Use this response to verify that the key has the expected access to the library you're trying to access. If necessary, show a warning that the user no longer has sufficient access and offer to remove a local library or reset local changes.

### 2) Get updated group metadata

Group metadata includes group titles and descriptions as well as member/role/permissions information. It is separate from group library data.

First, retrieve a list of the user's groups, with a version indicating the current state of each group's metadata:

    GET /users/<userID>/groups?format=versions

`200` Response:

    {
      "<groupID>": "<version>",
      "<groupID>": "<version>",
      "<groupID>": "<version>"
    }

Delete any local groups not in the list, which either were deleted or are currently inaccessible. (The user may have been removed from a group, or the current API key may no longer have access.) If data has been modified locally in any groups that are no longer available, offer the user the ability to cancel and transfer modified data elsewhere before continuing.

For each group that doesn't exist locally or that has a different version number, retrieve the group metadata:

    GET /groups/<groupID>

    Last-Modified-Version: <version>
    JSON response with metadata

Update the local group metadata and version number.

### 3) Sync library data

Perform the following steps for each library:

#### i. Get updated data

**Note:** Clients with changes to upload should attempt to [upload data](#iv_upload_modified_data) first and retrieve updated data only if they receive a `412 Precondition Failed`. See [If-Unmodified-Since-Version](#if-unmodified-since-version) for more information.

Retrieve the versions of all objects changed since the last check for that object type, using the appropriate request for each object type:

    GET <userOrGroupPrefix>/collections?since=<version>&format=versions
    GET <userOrGroupPrefix>/searches?since=<version>&format=versions
    GET <userOrGroupPrefix>/items/top?since=<version>&format=versions&includeTrashed=1
    GET <userOrGroupPrefix>/items?since=<version>&format=versions&includeTrashed=1

`<version>` is the final `Last-Modified-Version` returned from the API for the last successfully completed sync process, or `0` when syncing a library for the first time.

(The `since` parameter can also be used on `.../tags` requests (without `format=versions`) by clients that don't download all items and wish to keep a list of all tags in a library up-to-date. It isn't necessary for clients that download all items to request updated tags directly, as item objects contain all associated tags.)

The first request — e.g., for collection versions — can also include an `If-Modified-Since-Version: <last saved library version>` header. If the API returns `304 Not Modified`, no library data of any object type has changed since the version specified and no further requests need to be made to retrieve data unless there are [previously failed objects](#Handling save errors) that should be retried.

`200` response:

    Last-Modified-Version: <version>

    [
        "<objectKey>": <version>,
        "<objectKey>": <version>
        "<objectKey>": <version>,
    ]

For each returned object, compare the version to the local version of the object. If the remote version doesn't match, queue the object for download. Generally all returned objects should have newer version numbers, but there are some situations, such as full syncs (i.e., `since=0`) or interrupted syncs, where clients may retrieve versions for objects that are already up-to-date locally. The version will also match for top-level items on the second, non-`/top` `items` request, since top-level items will have already been processed.

Retrieve the queued objects, as well as any [flagged](#Handling save errors) as having previously failed to save, by key, up to 50 at a time, using the appropriate request for each object type:

    GET <userOrGroupPrefix>/collections?collectionKey=<key>,<key>,<key>,<key>
    GET <userOrGroupPrefix>/searches?searchKey=<key>,<key>,<key>,<key>
    GET <userOrGroupPrefix>/items?itemKey=<key>,<key>,<key>,<key>&includeTrashed=1

Item responses include creators, tags, collection associations, and relations.

Process the remote changes:

    for each updated object:
      if object doesn't exist locally:
         create local object with version = Last-Modified-Version and set synced = true
         continue

      if object hasn't been modified locally (synced == true):
          overwrite with synced = true and version = Last-Modified-Version

      else:
        perform conflict resolution
          if object hasn't changed:
            set synced = true and version = Last-Modified-Version

          else if changes can be automatically merged:
            apply changes from each side and set synced = true and version = Last-Modified-Version

          else:
            prompt user to choose a side or merge conflicts
              if user chooses remote copy:
                overwrite with synced = true and version = Last-Modified-Version

              else if user chooses local copy:
                synced = false and set a flag to restart the sync when finished


##### Conflict resolution

Conflict resolution is a complex process not fully described here, but see the Zotero client code for examples.

A few notable features:

1.  When an object is successfully downloaded or upload, the Zotero client saves the `data` block from the API response as pristine JSON tied to the object version. When a conflict occurs during a sync, it can then compare both the local and remote versions of the object to the pristine JSON to determine which changes were made on each side and automatically merge changes that aren't in conflict. Users are prompted to manually resolve only conflicting changes to the same field.
2.  The Zotero client automatically resolves conflicts for objects other than items without prompting the user, erring on the side of restoring deleted data.
3.  Restoring locally deleted collections is a special case. Item membership is a property of items, so no local items will still be a member of the collection after it's restored, and the local items also may have been deleted along with the collection. When restoring a locally deleted collection, the Zotero client fetches the collection's items from the API and either adds them back to the collection and marks them as unsynced (if they still exist locally) or removes them from the local delete log and flags them for manual download (if they don't).

##### Handling save errors

If an error occurs while processing an object (e.g., due to a foreign-key constraint in the local database), it can be handled one of two ways:

1.  Treat the error as fatal and stop the sync without updating the local library version
2.  Add the object key to a list of objects needing to be downloaded later and continue with the sync, updating the local library version at the end as if the sync had succeeded. In a future sync, add objects on this list to the set of objects returned from the `versions` request so that their data is requested again even if the remote version is lower than the library version specified in `?since=`. Retry these objects on a backoff schedule, since they may require either a server-side fix or a client update to save successfully. Skip objects in this list when uploading locally changed objects, since they are known to be out of date and would result in `412` errors. If these objects later appear as remotely deleted, remove them from the list of objects.

When processing a set of objects, it may be helpful to maintain a process queue for the sync run and move failing objects to the end of the queue in case they depend on other objects being retrieved. (In many cases, it's possible to sort objects beforehand to avoid such errors, such as by sorting parent collections before subcollections.) If a loop of the process queue completes without any objects being successfully processed, stop the sync.

#### ii. Get deleted data

    GET <userOrGroupPrefix>/deleted?since=<version>

`<version>` is, as above, the `Last-Modified-Version` returned from the API during the last successful sync run.

Response:

    Content-Type: application/json
    Last-Modified-Version: <version>

    {
      "collections": [
        "<collectionKey>"
      ],
      "searches": [
        "<searchKey>"
      ],
      "items": [
        "<itemKey>",
        "<itemKey>"
      ],
      "tags": [
        "<tagName>",
        "<tagName>"
      ]
    }

Process the remote deletions:

    for each deleted object in ['collections', 'searches', 'items']:
      if local object doesn't exist:
        continue

      if object hasn't been modified locally (synced == true):
        delete local object, skipping delete log

      else:
        perform conflict resolution
          if user chooses deletion, delete local object, skipping delete log

          if user chooses local modification, keep object and set synced = true and version = Last-Modified-Version

The Zotero client automatically resolves conflicts for objects other than items without prompting the user, erring on the side of restoring deleted data.

#### iii. Check for concurrent remote updates

For each response from the API, check the `Last-Modified-Version` to see if it has changed since the `Last-Modified-Version` returned from the first request (e.g., `collections?since=`). If it has, restart the process of retrieving updated and deleted data, waiting increasing amounts of time between restarts to give the other client the opportunity to finish.

After saving all remote changes without the remote version changing during the process, save `Last-Modified-Version` from the last run as the new local library version.

#### iv. Upload modified data

Upload objects which have `synced` set to `false`. Follow the instructions in [Updating Multiple Objects](dev/web_api/v3/write_requests#updating_multiple_objects), passing the current library version as `If-Unmodified-Since-Version`.

Creators, tags, and relations are included in item objects and are not synced separately.

On a `200` response, set `synced = true` and `version = Last-Modified-Version` for each successfully uploaded Zotero object and store `Last-Modified-Version` as the current library version to be passed with the next write request. Do not update the version of Zotero objects in the `unchanged` object. Retry non-fatal failures.

On a `412 Precondition Failed` response, return to the beginning of the sync process for that library, waiting increasing amounts of time between restarts.

#### v. Upload local deletions

When an object is deleted locally during regular usage, add its library and key to a delete log. When syncing, send delete requests for objects in the log and clear them from the log on successful deletion. When resolving a conflict between a locally deleted object and a remotely modified object in favor of the remote object, remove it from the delete log.

See [Deleting Multiple Collections](dev/web_api/v3/write_requests#deleting_multiple_collections), [Deleting Multiple Searches](dev/web_api/v3/write_requests#deleting_multiple_searches), and [Deleting Multiple Items](dev/web_api/v3/write_requests#deleting_multiple_items) for the specific requests. Pass the current library version as `If-Unmodified-Since-Version`.

Example request:

    DELETE <userOrGroupPrefix>/collections?collectionKey=<key>,<key>,<key>
    If-Unmodified-Since-Version: <version>

Response:

    204 No Content
    Last-Modified-Version: <version>

On a `204` response, store the returned `Last-Modified-Version` as the current library version to be passed with the next write request.

On a `412 Precondition Failed` response, return to the beginning of the sync process for that library.

## Partial-Library Syncing

The steps above are designed for clients that, after syncing, should always contain a complete local copy of a user's Zotero data. While this may make sense for permanently installed clients, it is less ideal for other use cases, such as for clients that provide temporary access to a library or that will often be connected via mobile connections where downloading all data in a library would be prohibitively slow or expensive. Selective syncing requires some modifications to the above steps. Three possible approaches are outlined below:

### Fixed Collection List

This approach would work for a client that allowed users to choose a subset of collections to sync but otherwise behaved like a full offline client.

The client would still need to track only a single library version, but instead of downloading a list of all items from `<userOrGroupPrefix>/items?format=versions&since=<version>`, it would retrieve the list of items from each selected collection individually with requests such as `<userOrGroupPrefix>/collections/<collectionKey/items?format=versions&since=<version>`. The local library version would be updated only once the items in all collections had been downloaded (or queued for download in a persistent fashion).

### Per-Collection Versions

This approach would work for a client that loaded data only in response to user interaction — such as clicking on a collection — rather than loading a predefined set of collections.

The client would need to track separate library versions for each view that represented the state of all objects within that view. If an upload to a multi-object endpoint such as `<userOrGroupPrefix>/items` resulted in a `412`, indicating that something in the library — though not necessarily in the view — had changed, the client would need to fetch only the new data (or the list of objects containing new data) in the view and update the version number associated with the view. Note that such a version number would be separate from the version number of the view object — for example, the collection — itself.

Clients would also need to keep track of a version number that represented the state of the collection/search list. (While they could simply reload the entire collections list, doing so would be slow for users with many collections.)

### Single-Object Versions

A final approach would be to eschew library-wide version numbers altogether and use only single-object versions to upload data. This could be done via the single-object endpoints using the `If-Unmodified-Since-Version` header or via multi-object endpoints using the JSON version properties. As an object's editable JSON includes the object version, clients that pass the received JSON back to the server will get safe updates automatically. This can be thought of as the default API usage mode.

Note that multi-object endpoints should always be used for large operations. Using single-object endpoints excessively could result in throttling by the server.

# Zotero Streaming API

The Zotero streaming API provides push-based notifications via WebSockets for Zotero library changes, allowing for nearly instantaneous updates when data changes in a library or when a user joins or leaves a library.

Note that this API provides library-level notifications of changes. It does not provide updated data directly. API consumers that receive notification of a library change should use their standard [sync process](dev/web_api/v3/syncing) to retrieve data, ensuring a single, consistent code path for both manual and automatic syncing.

To avoid missed updates, clients should connect to the streaming API and then, once connected, trigger a standard sync operation to bring themselves up to date with the current version of a library.

## Requests

### Create an empty WebSocket stream

    var ws = new WebSocket('wss://stream.zotero.org');

Server response:

    {"event": "connected", "retry": 10000}

### Add subscriptions to the event stream

Client message:

    {
        "action": "createSubscriptions",
        "subscriptions": [
            {
                "apiKey": "abcdefghijklmn1234567890",
                "topics": [
                    "/users/123456",
                    "/groups/234567",
                    "/groups/345678"
                ]
            },
            {
                "apiKey": "bcdefghijklmn12345678901"
            },
            {
                "topics": [
                    "/groups/456789",
                    "/groups/567890"
                ]
            }
        ]
    }

Server Response:

    {
        "event": "subscriptionsCreated",
        "subscriptions": [
            {
                "apiKey": "abcdefghijklmn1234567890",
                "topics": [
                    "/users/123456",
                    "/groups/234567"
                ]
            },
            {
                "apiKey": "bcdefghijklmn2345678901",
                "topics": [
                    "/users/345678"
                ]
            },
            {
                "topics": [
                    "/groups/456789"
                ]
            }
        ],
        "errors": [
            {
                "apiKey": "abcdefghijklmn1234567890",
                "topic": "/groups/345678",
                "error": "Topic is not valid for provided API key"
            },
            {
                "topic": "/groups/567890",
                "error": "Topic is not accessible without an API key"
            }
        ]
    }

All topic subscriptions — new and existing — for the specified API keys are included in the response. Subscriptions for previously added API keys not in the current request are not included. Subscriptions for public topics can be made without specifying an API key, and the newly added topics will be grouped together in the response.

If a `topics` property is not provided for an API key, the connection will receive events for all topics available to that key and will [automatically track](#key_access_tracking) changes to the key's available topics.

Topic subscriptions cannot be removed via `createSubscriptions`. If subscriptions for a given API key already exist, the provided topics will be merged with the existing ones. If an empty `topics` array is provided, no changes will be made. If no `topics` property is provided, the key will be upgraded to automatically track access as described above.

#### Errors

| | |
| :--- | :--- |
| `4403 Forbidden` | Invalid API key |
| `4413 Request Entity Too Large` | Number of subscriptions (including existing subscriptions) would exceed the per-connection limit |

### Receive events on the existing event stream

    {"event": "topicUpdated", "topic": "/users/123456", "version": 678}

    {"event": "topicAdded", "apiKey": "abcdefghijklmn1234567890", "topic": "/groups/345678"}

    {"event": "topicRemoved", "apiKey": "abcdefghijklmn1234567890", "topic": "/groups/234567"}

### Delete all subscriptions for a given API key

Client message:

    {
        "action": "deleteSubscriptions",
        "subscriptions": [
            {
                "apiKey": "abcdefghijklmn1234567890"
            }
        ]
    }

Server response:

    {
        "event": "subscriptionsDeleted"
    }

#### Errors

| | |
| :--- | :--- |
| `4409 Conflict` | Subscription with a given API key or topic doesn't exist on this connection |

### Delete specific API key/topic pair

Client message:

    {
        "action": "deleteSubscriptions",
        "subscriptions": [
            {
                "apiKey": "abcdefghijklmn1234567890",
                "topic": "/users/123456"
            }
        ]
    }

Server response:

    {
        "event": "subscriptionsDeleted"
    }

If a topic is manually removed from a key that is automatically tracking topics, the resulting list of topics will be fixed and the key will no longer receive `topicAdded` events. It may still receive `topicRemoved` events if the key loses access to topics.

#### Errors

| | |
| :--- | :--- |
| `4409 Conflict` | Subscription with the given API key and/or topic doesn't exist on this connection |

### Delete a public topic subscription

Client message:

    {
        "action": "deleteSubscriptions",
        "subscriptions": [
            {
                "topic": "/users/123456"
            }
        ]
    }

Server response:

    {
        "event": "subscriptionsDeleted"
    }

#### Errors

| | |
| :--- | :--- |
| `4409 Conflict` | Public subscription for the given topic doesn't exist on this connection |

## Key Access Tracking

For API keys without specified topics, the connection will track the key's access and receive events for all topics available to the key.

For example, if the owner of the key joins a group and the key has access to all of the user's groups, the connection will receive a `topicAdded` event and begin receiving `topicUpdated` events as data in the group changes.
