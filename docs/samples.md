# Sample requests and responses

Captured from a local `docker compose` stack on 2026-11-28 13:33 UTC by
`scripts/validate_responses.py --samples`. Every response below passed the openapi.yaml schema check.
Long arrays are cut to two items. Set `BASE`, `KEY` (group key), `TOKEN` (from login), `ADMIN_KEY`.

## health

```sh
curl -s -X GET "$BASE/health" 
```

`200`

```json
{
  "status": "ok"
}
```

## log in with the test account

```sh
curl -s -X POST "$BASE/auth/login"  \
  -H 'content-type: application/json' -d '{"email": "penguji@kotasiaga.test", "password": "<test account password>"}'
```

`200`

```json
{
  "data": {
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5...(jwt)",
    "expires_in": 3600,
    "refresh_expires_in": 2592000,
    "refresh_token": "rt_8b67e84...(refresh token)",
    "token_type": "bearer",
    "user": {
      "display_name": "Akun Uji KotaSiaga",
      "email": "penguji@kotasiaga.test",
      "id": "usr_152826fa5b994adfa2b5"
    }
  }
}
```

## me

```sh
curl -s -X GET "$BASE/auth/me" -H "Authorization: Bearer $TOKEN"
```

`200`

```json
{
  "data": {
    "display_name": "Akun Uji KotaSiaga",
    "email": "penguji@kotasiaga.test",
    "id": "usr_152826fa5b994adfa2b5"
  }
}
```

## register a second test user

```sh
curl -s -X POST "$BASE/auth/register"  \
  -H 'content-type: application/json' -d '{"email": "warga-01f22e97@kotasiaga.test", "password": "<password>", "display_name": "Warga Uji"}'
```

`201`

```json
{
  "data": {
    "display_name": "Warga Uji",
    "email": "warga-01f22e97@kotasiaga.test",
    "id": "usr_fa2075983f1e48688024"
  }
}
```

## refresh

```sh
curl -s -X POST "$BASE/auth/refresh"  \
  -H 'content-type: application/json' -d '{"refresh_token": "<refresh_token>"}'
```

`200`

```json
{
  "data": {
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5...(jwt)",
    "expires_in": 3600,
    "refresh_expires_in": 2592000,
    "refresh_token": "rt_946c5f9...(refresh token)",
    "token_type": "bearer",
    "user": {
      "display_name": "Warga Uji",
      "email": "warga-01f22e97@kotasiaga.test",
      "id": "usr_fa2075983f1e48688024"
    }
  }
}
```

## status

```sh
curl -s -X GET "$BASE/status?lat=-6.9&lon=107.6&radius_m=2000" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": {
    "cell_id": "c_44_33",
    "risk_level": "safe",
    "weather": {
      "observed_at": "2026-11-28T13:33:57.178255168Z",
      "temperature_c": 29.3,
      "rain_mm_h": 3.2,
      "condition": "rain",
      "forecast_6h": [
        {
          "at": "2026-11-28T14:33:57.178255168Z",
          "rain_mm_h": 1.6,
          "temperature_c": 29.5
        },
        {
          "at": "2026-11-28T15:33:57.178255168Z",
          "rain_mm_h": 0.8,
          "temperature_c": 29.6
        },
        "..."
      ]
    },
    "active_reports": 0,
    "disaster_signals": 0
  },
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## risk of one cell

```sh
curl -s -X GET "$BASE/risk?lat=-6.89&lon=107.61" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": {
    "cell_id": "c_46_35",
    "factors": [
      {
        "type": "rain",
        "value": 3.2
      },
      {
        "count": 0,
        "type": "verified_report"
      },
      "..."
    ],
    "risk_level": "safe",
    "score": 0.03
  },
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## weather

```sh
curl -s -X GET "$BASE/weather?lat=-6.89&lon=107.61" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": {
    "observed_at": "2026-11-28T13:33:57.178255168Z",
    "temperature_c": 29.3,
    "rain_mm_h": 3.2,
    "condition": "rain",
    "forecast_6h": [
      {
        "at": "2026-11-28T14:33:57.178255168Z",
        "rain_mm_h": 1.6,
        "temperature_c": 29.5
      },
      {
        "at": "2026-11-28T15:33:57.178255168Z",
        "rain_mm_h": 0.8,
        "temperature_c": 29.6
      },
      "..."
    ]
  },
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## disasters

```sh
curl -s -X GET "$BASE/disasters?limit=5" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": [
    {
      "id": "dis_1",
      "type": "flood",
      "lat": -6.920255029032895,
      "lon": 107.6468688293279,
      "reported_at": "2026-11-28T13:33:17.175121610Z",
      "status": "active",
      "resolved_at": null
    }
  ],
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## reports

```sh
curl -s -X GET "$BASE/reports?limit=3" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": [
    {
      "id": "rpt_60",
      "category": "accident",
      "status": "waiting",
      "lat": -6.925714498784389,
      "lon": 107.70817811511903,
      "description": "Tabrakan dua motor",
      "severity": 2,
      "confirm_count": 0,
      "dispute_count": 0,
      "created_at": "2026-11-28T13:33:57.178255168Z",
      "verified_at": null,
      "expires_at": "2026-11-28T13:35:57.178255168Z"
    },
    {
      "id": "rpt_59",
      "category": "road_damage",
      "status": "waiting",
      "lat": -6.949651726334524,
      "lon": 107.63554189463085,
      "description": "Aspal retak dan amblas",
      "severity": 1,
      "confirm_count": 0,
      "dispute_count": 0,
      "created_at": "2026-11-28T13:33:52.177606396Z",
      "verified_at": null,
      "expires_at": "2026-11-28T14:03:52.177606396Z"
    },
    "..."
  ],
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## changes

```sh
curl -s -X GET "$BASE/changes?since=0&limit=3" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": [
    {
      "seq": 1,
      "at": "2026-11-28T13:32:52.170917096Z",
      "type": "weather.updated",
      "entity_id": "bandung",
      "summary": "Hujan 0.5 mm/jam"
    },
    {
      "seq": 2,
      "at": "2026-11-28T13:32:52.172276656Z",
      "type": "report.expired",
      "entity_id": "rpt_1",
      "summary": "Genangan kedaluwarsa"
    },
    "..."
  ],
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## monitor points (Tourism)

```sh
curl -s -X GET "$BASE/monitor-points?limit=3" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": [
    {
      "id": "tour_1",
      "name": "Gedung Sate",
      "category": "culture_history",
      "lat": -6.9025,
      "lon": 107.6188,
      "cell_id": "c_43_37",
      "risk_level": "safe",
      "rain_mm_h": 3.2
    },
    {
      "id": "tour_2",
      "name": "Jalan Braga",
      "category": "culture_history",
      "lat": -6.9175,
      "lon": 107.6093,
      "cell_id": "c_40_35",
      "risk_level": "safe",
      "rain_mm_h": 3.2
    },
    "..."
  ],
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## destination points (Health)

```sh
curl -s -X GET "$BASE/destination-points?limit=3" -H "X-API-Key: $KEY"
```

`200`

```json
{
  "data": [
    {
      "id": "dnr_evt_1",
      "name": "Donor Darah UDD PMI Kota Bandung",
      "kind": "donation_event",
      "lat": -6.9097,
      "lon": 107.6175,
      "opens_at": "2026-11-29T01:00:00Z",
      "closes_at": "2026-11-29T06:00:00Z",
      "slots_available": 28,
      "cell_id": "c_42_37",
      "risk_level": "safe",
      "rain_mm_h": 3.2
    },
    {
      "id": "dnr_evt_2",
      "name": "Donor Darah RSUP Dr. Hasan Sadikin",
      "kind": "donation_event",
      "lat": -6.8945,
      "lon": 107.5985,
      "opens_at": "2026-11-29T01:30:00Z",
      "closes_at": "2026-11-29T05:30:00Z",
      "slots_available": 23,
      "cell_id": "c_45_32",
      "risk_level": "safe",
      "rain_mm_h": 3.2
    },
    "..."
  ],
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## safe route

```sh
curl -s -X POST "$BASE/routes/safe" -H "X-API-Key: $KEY" \
  -H 'content-type: application/json' -d '{"origin": {"lat": -6.9025, "lon": 107.6188}, "destination": {"lat": -6.9218, "lon": 107.607}, "alternatives": 3}'
```

`200`

```json
{
  "data": {
    "recommended_id": "r1",
    "routes": [
      {
        "id": "r1",
        "distance_m": 2513.0,
        "duration_s": 303.0,
        "risk_penalty": 0.03,
        "max_risk_level": "safe",
        "geometry": {
          "type": "LineString",
          "coordinates": [
            [
              107.6188,
              -6.9025
            ],
            [
              107.61830833333335,
              -6.9033041666666675
            ],
            [
              107.61781666666666,
              -6.904108333333332
            ],
            [
              "..."
            ]
          ]
        },
        "reasons": [
          "Rute tercepat",
          "Tidak ada risiko tercatat di jalur ini"
        ]
      },
      {
        "id": "r2",
        "distance_m": 2592.0,
        "duration_s": 312.0,
        "risk_penalty": 0.03,
        "max_risk_level": "safe",
        "geometry": {
          "type": "LineString",
          "coordinates": [
            [
              107.6188,
              -6.9025
            ],
            [
              107.61796924305557,
              -6.903096847222224
            ],
            [
              107.6171679722222,
              -6.903711722222221
            ],
            [
              "..."
            ]
          ]
        },
        "reasons": [
          "Tidak ada risiko tercatat di jalur ini"
        ]
      },
      "..."
    ]
  },
  "meta": {
    "seq": 40,
    "updated_at": "2026-11-28T13:33:57.178255168Z",
    "source": "mock",
    "stale": false
  }
}
```

## create a report

```sh
curl -s -X POST "$BASE/reports" -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"category": "flood", "lat": -6.9175, "lon": 107.6093, "description": "Genangan 20 cm di Jl. Braga", "severity": 2}'
```

`201`

```json
{
  "data": {
    "id": "rpt_61",
    "category": "flood",
    "status": "waiting",
    "lat": -6.9175,
    "lon": 107.6093,
    "description": "Genangan 20 cm di Jl. Braga",
    "severity": 2,
    "confirm_count": 0,
    "dispute_count": 0,
    "created_at": "2026-11-28T13:33:58.007006411Z",
    "verified_at": null,
    "expires_at": "2026-11-28T13:36:58.007006411Z"
  },
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## confirm someone else's report

```sh
curl -s -X POST "$BASE/reports/rpt_61/votes" -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"vote": "confirm"}'
```

`200`

```json
{
  "data": {
    "id": "rpt_61",
    "category": "flood",
    "status": "waiting",
    "lat": -6.9175,
    "lon": 107.6093,
    "description": "Genangan 20 cm di Jl. Braga",
    "severity": 2,
    "confirm_count": 1,
    "dispute_count": 0,
    "created_at": "2026-11-28T13:33:58.007006411Z",
    "verified_at": null,
    "expires_at": "2026-11-28T13:36:58.007006411Z"
  },
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## subscribe to an area

```sh
curl -s -X POST "$BASE/subscriptions" -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"name": "Kampus", "lat": -6.8915, "lon": 107.6107, "radius_m": 1000}'
```

`201`

```json
{
  "data": {
    "id": "sub_4b44922e2da54883b5b1",
    "name": "Kampus",
    "lat": -6.8915,
    "lon": 107.6107,
    "radius_m": 1000,
    "created_at": "2026-11-28T13:33:58.017972371Z",
    "risk_level": "safe"
  },
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## list subscriptions

```sh
curl -s -X GET "$BASE/subscriptions" -H "Authorization: Bearer $TOKEN"
```

`200`

```json
{
  "data": [
    {
      "id": "sub_4b44922e2da54883b5b1",
      "name": "Kampus",
      "lat": -6.8915,
      "lon": 107.6107,
      "radius_m": 1000,
      "created_at": "2026-11-28T13:33:58.017Z",
      "risk_level": "safe"
    }
  ],
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## alerts

```sh
curl -s -X GET "$BASE/alerts?limit=5" -H "Authorization: Bearer $TOKEN"
```

`200`

```json
{
  "data": [],
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## mock state

```sh
curl -s -X GET "$BASE/_mock/state" -H "X-API-Key: $ADMIN_KEY"
```

`200`

```json
{
  "data": {
    "seed": 11,
    "tick_seconds": 5,
    "time_scale": 60,
    "tick": 13,
    "active_scenario": null,
    "live_reports": 14,
    "active_disasters": 1,
    "upstreams_down": []
  },
  "meta": {
    "seq": 41,
    "updated_at": "2026-11-28T13:33:58.007006411Z",
    "source": "mock",
    "stale": false
  }
}
```

## mock scenario

```sh
curl -s -X POST "$BASE/_mock/scenario" -H "X-API-Key: $ADMIN_KEY" \
  -H 'content-type: application/json' -d '{"name": "flood_verified", "lat": -6.8915, "lon": 107.6107}'
```

`202`

```json
{
  "data": {
    "seed": 11,
    "tick_seconds": 5,
    "time_scale": 60,
    "tick": 13,
    "active_scenario": "flood_verified",
    "live_reports": 15,
    "active_disasters": 1,
    "upstreams_down": []
  },
  "meta": {
    "seq": 44,
    "updated_at": "2026-11-28T13:33:58.025945860Z",
    "source": "mock",
    "stale": false
  }
}
```

## delete the subscription

```sh
curl -s -X DELETE "$BASE/subscriptions/sub_4b44922e2da54883b5b1" -H "Authorization: Bearer $TOKEN"
```

`204`

## logout

```sh
curl -s -X POST "$BASE/auth/logout"  \
  -H 'content-type: application/json' -d '{"refresh_token": "<refresh_token>"}'
```

`204`

