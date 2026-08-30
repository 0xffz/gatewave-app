# Live probe fixtures (captured 2026-08-30)

Verbatim response bodies from each provider's `handler_api.php`, one file per request.
File name = `<action>[_<param>_<value>...]`. Requests that spend money (`getNumber*`) were NOT made.

## Hero-SMS — `https://hero-sms.com/stubs/handler_api.php` (HTTP status in brackets)

| file | status | note |
| --- | --- | --- |
| getBalance.txt | 200 | `ACCESS_BALANCE:12.5085` |
| getCountries.txt | 200 | `{id:{id:int,rus,eng,chn,visible,retry,rent}}` |
| getServicesList.txt | 200 | `{status,services:[{code,name}]}` |
| getServices.txt | 404 | `BAD_ACTION` JSON envelope — action does not exist |
| getPrices.txt / getPrices_service_tg.txt | 200 | `{country:{service:{cost,count,physicalCount}}}` |
| getPrices_service_tg_country_0.txt | 200 | `{}` (empty) |
| getPricesV2_*.txt / getPricesV3_*.txt | 404 | not supported |
| getTopCountriesByService_service_tg.txt | 200 | `{idx:{country,price,retail_price,count}}` |
| getTopCountriesByService_service_tg_freePrice_true.txt | 200 | adds `freePriceMap` |
| getNumbersStatus_country_73.txt / _187.txt | 200 | `{service_code:count}` |
| getNumbersStatus_country_0.txt | 200 | `{}` |
| getNumbersStatus.txt | 422 | validation envelope, `country` required |
| getOperators_country_73.txt | 200 | `{status,countryOperators:{country:[...]}}` |
| getActiveActivations.txt | 200 | `{status,data:[],activeActivations:{...}}` (no live activations at capture time) |
| getStatus_id_1.txt / getStatus_id_999999999999.txt / getStatusV2_id_1.txt / setStatus_id_1_status_8.txt | 404 | `{"title":"NOT_FOUND","details":"Activation Not Found"}` |
| getStatus.txt / getStatus_id_abc.txt | 422 | validation envelope, `id` invalid |
| getFullSms_id_1.txt / getBalanceX.txt | 404 | `{"title":"BAD_ACTION","details":"Method Not Found"}` |
| badkey_getBalance.txt | 401 | `{"title":"BAD_KEY","details":"Unauthorized"}` |
| ratelimit_429.txt | 429 | `{"title":"RATE_LIMIT","details":""}` — seen after a burst of requests |
| nokey_getBalance.txt | 422 | validation envelope, `api_key` required |
| getNumberV2_service___probe___country_187.txt | 422 | invalid service → validation envelope (`field: service`) — proves `getNumberV2` exists; nothing was bought |
| getNumber_service___probe___country_187.txt | 200 | **plain text** `UNPROCESSABLE_ENTITY:service:INVALID` — v1 validation is a token, not JSON |
| getAllSms_id_1.txt / cancelActivation_id_1.txt | 404 | `NOT_FOUND` envelope — both actions exist |
| getOperators.txt | 200 | all countries (137) — `{status,countryOperators:{country:[...]}}` |
| getServicesList_country_187_lang_ru.txt | 200 | `country` filters the list (262 rows), `lang` accepted |
| getTopCountriesByService_excerpt.txt | 200 | first two services of the 1.35 MB no-service answer: `{service:{idx:{country,price,retail_price,count}}}` |

## SMSBower — `https://smsbower.page/stubs/handler_api.php` (GET only; POST → 405)

| file | status | note |
| --- | --- | --- |
| getBalance.txt | 200 | `ACCESS_BALANCE:18.739` |
| getCountries.txt | 200 | `{id:{id:"string",rus,eng,chn}}` |
| getServicesList.txt | 200 | `{status,services:[{code,name}]}` |
| getPrices_service_tg.txt / getPrices_service_tg_country_187.txt | 200 | `{country:{service:{cost,count}}}` |
| getPricesV2_service_tg_country_187.txt | 200 | `{country:{service:{"<price>":count}}}` |
| getPricesV3_service_tg_country_187.txt | 200 | `{country:{service:{providerId:{count,price,provider_id}}}}` |
| getTopCountriesByService_service_tg.txt | 200 | `{"<country-slug>":{providerId:{price,count}}}` |
| getStatus_id_1.txt / getStatus_id_abc.txt / setStatus_id_1_status_8.txt | 200 | plain `NO_ACTIVATION` |
| getNumbersStatus_country_187.txt / getActiveActivations.txt / getFullSms_id_1.txt / nosuchaction.txt | 200 | plain `BAD_ACTION` |
| badkey_getBalance.txt | 401 | `{"status":0,"message":"No access","data":[]}` |
| post_getBalance.txt | 405 | POST rejected |
| getOperators_country_187.txt | 200 | `{status,countryOperators:{"187":[]}}` — undocumented but supported |
| getNumber_service___probe___country_187.txt / getNumberV2_service___probe___country_187.txt | 200 | plain `WRONG_SERVICE` for the invalid service `__probe__` (docs say `BAD_SERVICE`); proves both actions exist, no purchase made |
| getPrices_service_tg_country_9999.txt / getPricesV2_service_tg_country_9999.txt | 200 | `{"error":"Bad country"}` — JSON envelope with HTTP 200 for an unknown country |
| getPrices_service___probe___country_187.txt / getPricesV2_service___probe___country_187.txt | 200 | `{"error":"Bad service"}` — same envelope for an unknown service |
| getPricesV3_service_tg_country_9999.txt / getPricesV3_service___probe___country_187.txt | 200 | plain `BAD_COUNTRY` / `BAD_SERVICE` — V3 alone uses the documented tokens |
| getTopCountriesByService_service___probe__.txt | 200 | `{"error":"BAD_SERVICE"}` — the token inside the JSON envelope |
| getOperators_country_9999.txt | 200 | `{"status":"success","countryOperators":[]}` — PHP empty array for an unknown country |

## 5SIM — `https://5sim.net/v1` (own JSON REST API, `Authorization: Bearer <token>`)

Guest endpoints need no key. `/user/*` without/with a bad token → HTTP 401, empty body.

| file | status | note |
| --- | --- | --- |
| guest_countries.json | 200 | `{"england":{"iso":{"gb":1},"prefix":{"+44":1},"text_en","text_ru","<operator>":{"activation":1},…}}` |
| guest_products_england_any.json | 200 | `/guest/products/england/any` → `{"<product>":{"Category":"activation"\|"hosting","Qty","Price"}}` |
| guest_prices_product_telegram.json | 200 | `/guest/prices?product=telegram` → `{"telegram":{"<country>":{"<operator>":{"cost","count","rate"…}}}}` |
| guest_prices_country_england_product_telegram.json | 200 | `/guest/prices?country=england&product=telegram` → `{"england":{"telegram":{"<operator>":{…}}}}` |
| guest_prices_country_england.json | 200 | `/guest/prices?country=england`, SLICED to 12 products (original 627 KB) |
| user_profile.txt | 200 | `{id,email,balance,rating,default_country,default_operator,frozen_balance,…}` plus undocumented `did_order`, `is_totp`, `last_order`, `last_top_idx`, `last_top_orders` (a string), `total_active_orders` — id/email redacted |
| user_orders_category_activation_limit_5_offset_0_order_id_reverse_true.txt | 200 | `{Data:[{id,phone,operator,product,price,status,expires,sms:[…],created_at,country}],ProductNames,Statuses,Total}` — phones/texts redacted; `Total` was 999999 |
| user_payments_limit_3_offset_0_order_id_reverse_true.txt | 200 | `{Data:[{ID,TypeName,ProviderName,Amount,Balance,CreatedAt}],PaymentTypes,PaymentProviders,Total}` |
| user_max-prices.txt | 200 | `[]` — no price limits set |
| user_check_1.txt | 404 | plain `order not found` |
| user_cancel_1.txt / user_finish_1.txt / user_ban_1.txt | 400 | plain `order not found` — note 400 here vs 404 on `check` |
| user_buy_activation_england_any_zzprobezz.txt | 400 | plain `no product` — the only buy call, an unknown product; nothing bought |
| user_buy_activation_england_any___probe__.txt | 302 | `<a href="/404.html">Found</a>.` — `__probe__` does not match the route pattern; redirect to the HTML 404 page |
| user_sms_inbox_1.txt | 302 | same redirect — the documented `/user/sms/inbox/{id}` route answered 302 → `/404.html` for ids 1 and 999999999 |
| guest_prices_country_england_product___probe__.txt | 400 | plain `product is incorrect` |
| guest_prices_country___probe___product_telegram.txt | 400 | plain `country is incorrect` |
| guest_products___probe___any.txt | 400 | plain `bad country` |

`/guest/countries` with a bogus bearer still answers 200 (guest endpoints ignore the header).

Not captured: `/guest/prices` (9 MB), `/guest/flags/<country>` (302 → 404 page).

## Tiger SMS — `https://api.tiger-sms.com/stubs/handler_api.php` (also `https://tiger-sms.com/stubs/handler_api.php`; GET and POST both work)

OpenAPI spec: `openapi.json` (from https://tiger-sms.com/api/openapi.json). Same backend family as Hero-SMS (identical `NOT_FOUND` envelope on `getStatusV2`).

| file | status | note |
| --- | --- | --- |
| getBalance.txt | 200 | `ACCESS_BALANCE:4.682` |
| getCountries.txt | 200 | **ARRAY** `[{"id":74,"rus","eng","chn","visible":1,"retry":1},…]` (no `rent`) |
| getServicesList.txt | 200 | `{status,services:[{code,name}]}` |
| getServices.txt / getTopCountriesByService_service_tg.txt / getNumbersStatus_country_187.txt / getOperators_country_187.txt / getFullSms_id_1.txt / nosuchaction.txt | 200 | plain `BAD_ACTION` — not supported |
| getPrices_service_tg.txt / getPrices_service_tg_country_187.txt | 200 | `{country:{service:{cost:"0.2500" (STRING),count}}}` |
| getPricesV2_service_tg_country_187.txt | 200 | `{country:{service:{prices:{"<price>":count},has_multi:{"<price>":bool}}}}` |
| getPricesV3_service_tg_country_187.txt | 200 | `{country:{service:{price,count,currency:840,saleAveragePrice,providers:{id:{count,price:[…],provider_id}}}}}` |
| getActiveActivations.txt | 200 | `{"status":"success","data":[]}` |
| getStatus_id_1.txt / getStatus_id_abc.txt / setStatus_id_1_status_8.txt | 200 | plain `NO_ACTIVATION` |
| getStatusV2_id_1.txt | 404 | `{"title":"NOT_FOUND","details":"Activation Not Found"}` |
| badkey_getBalance.txt | 401 | plain `BAD_KEY` |
| post_getBalance.txt | 200 | POST works: `ACCESS_BALANCE:4.682` |
| getNumberV2_service___probe___country_187.txt | **200** | `{"title":"BAD_SERVICE","details":"This service/country combination is not available"}` — JSON envelope with HTTP 200; proves the action exists, no purchase made |
| getNumber_service___probe___country_187.txt | 200 | plain `BAD_SERVICE` (v1 exists, no purchase made) |
| getFreePrices_service_tg_country_187.txt | 200 | byte-identical to `getPricesV2_*` (documented alias); ladders are cumulative |
| getOffers_services_tg_countries_187.txt | 200 | `{"data":{service:{country:{prices:{default,avg,retail,min},counts:{total,defaultPrice},map:{"<price>":count}}}}}` — service first |
| getProviders_service_tg_country_187.txt | 200 | `<html>…<body>[{id,name:"Provider<id>",numbers_count,delivery_percent (may be null),number_lifetime}]</body></html>` |
| getServiceNumbersCount_service_tg.txt | 200 | `[{"countryCode":187 (NUMBER; docs show a string),"numbersCount":44432},…]` |
| getBalance_format_json.txt | 200 | `{"balance":"4.6820","currency":840}` |
| setStatusV2_id_1_status_8.txt | 404 | `{"title":"NOT_FOUND","details":"Activation Not Found"}` |
| getServicesList_country_187_lang_ru.txt | 200 | `country=187` filters (977 → 580 services); names stayed English despite `lang=ru` |
| getActiveActivations_start_0_limit_1.txt | 200 | `{"status":"success","data":[]}` — `start`/`limit` accepted |
