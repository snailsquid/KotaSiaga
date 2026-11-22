# Sample requests and responses

Captured from a local `docker compose` stack on 2026-11-22 06:57 UTC by
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
    "refresh_token": "rt_b188dfb...(refresh token)",
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
  -H 'content-type: application/json' -d '{"email": "warga-a3a4b5ee@kotasiaga.test", "password": "<password>", "display_name": "Warga Uji"}'
```

`201`

```json
{
  "data": {
    "display_name": "Warga Uji",
    "email": "warga-a3a4b5ee@kotasiaga.test",
    "id": "usr_daa9f83467374b69aedb"
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
    "refresh_token": "rt_bb5a353...(refresh token)",
    "token_type": "bearer",
    "user": {
      "display_name": "Warga Uji",
      "email": "warga-a3a4b5ee@kotasiaga.test",
      "id": "usr_daa9f83467374b69aedb"
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
      "observed_at": "2026-11-22T06:57:38.699273625Z",
      "temperature_c": 28.7,
      "rain_mm_h": 9.0,
      "condition": "rain",
      "forecast_6h": [
        {
          "at": "2026-11-22T07:57:38.699273625Z",
          "rain_mm_h": 4.5,
          "temperature_c": 29.4
        },
        {
          "at": "2026-11-22T08:57:38.699273625Z",
          "rain_mm_h": 2.2,
          "temperature_c": 29.7
        },
        "..."
      ]
    },
    "active_reports": 0,
    "disaster_signals": 0
  },
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
        "value": 9.0
      },
      {
        "count": 0,
        "type": "verified_report"
      },
      "..."
    ],
    "risk_level": "safe",
    "score": 0.07
  },
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
    "observed_at": "2026-11-22T06:57:38.699273625Z",
    "temperature_c": 28.7,
    "rain_mm_h": 9.0,
    "condition": "rain",
    "forecast_6h": [
      {
        "at": "2026-11-22T07:57:38.699273625Z",
        "rain_mm_h": 4.5,
        "temperature_c": 29.4
      },
      {
        "at": "2026-11-22T08:57:38.699273625Z",
        "rain_mm_h": 2.2,
        "temperature_c": 29.7
      },
      "..."
    ]
  },
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
      "lat": -6.844927624097992,
      "lon": 107.64971213440222,
      "reported_at": "2026-11-22T06:57:18.693232293Z",
      "status": "active",
      "resolved_at": null
    },
    {
      "id": "dis_2",
      "type": "fire",
      "lat": -6.950721859879102,
      "lon": 107.63236083903274,
      "reported_at": "2026-11-22T06:57:38.699273625Z",
      "status": "active",
      "resolved_at": null
    }
  ],
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
      "id": "rpt_4",
      "category": "flood",
      "status": "waiting",
      "lat": -6.965075602618643,
      "lon": 107.54909923016125,
      "description": "Banjir di depan pertokoan",
      "severity": 2,
      "confirm_count": 2,
      "dispute_count": 1,
      "created_at": "2026-11-22T06:57:28.694497354Z",
      "verified_at": null,
      "expires_at": "2026-11-22T07:00:28.694497354Z"
    },
    {
      "id": "rpt_3",
      "category": "flood",
      "status": "waiting",
      "lat": -6.960049734032472,
      "lon": 107.63087680956909,
      "description": "Genangan setinggi mata kaki",
      "severity": 3,
      "confirm_count": 3,
      "dispute_count": 1,
      "created_at": "2026-11-22T06:57:13.697000826Z",
      "verified_at": null,
      "expires_at": "2026-11-22T07:00:13.697000826Z"
    },
    "..."
  ],
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
      "at": "2026-11-22T06:57:08.684345147Z",
      "type": "weather.updated",
      "entity_id": "bandung",
      "summary": "Hujan 2.9 mm/jam"
    },
    {
      "seq": 2,
      "at": "2026-11-22T06:57:13.697000826Z",
      "type": "weather.updated",
      "entity_id": "bandung",
      "summary": "Hujan 3.2 mm/jam"
    },
    "..."
  ],
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
      "id": "tour_dst_01",
      "name": "Gedung Sate",
      "category": "heritage",
      "lat": -6.9025,
      "lon": 107.6188,
      "cell_id": "c_43_37",
      "risk_level": "safe",
      "rain_mm_h": 9.0
    },
    {
      "id": "tour_dst_02",
      "name": "Alun-alun Bandung",
      "category": "public_space",
      "lat": -6.9218,
      "lon": 107.607,
      "cell_id": "c_39_34",
      "risk_level": "safe",
      "rain_mm_h": 9.0
    },
    "..."
  ],
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
      "id": "dnr_evt_001",
      "name": "Donor darah UDD PMI Kota Bandung, Jl. Aceh",
      "kind": "donation_event",
      "lat": -6.9097,
      "lon": 107.6175,
      "opens_at": "2026-11-22T01:00:00Z",
      "closes_at": "2026-11-22T08:00:00Z",
      "slots_available": 11,
      "cell_id": "c_42_37",
      "risk_level": "safe",
      "rain_mm_h": 9.0
    },
    {
      "id": "dnr_evt_002",
      "name": "Donor darah RSUP Dr. Hasan Sadikin",
      "kind": "donation_event",
      "lat": -6.8945,
      "lon": 107.5985,
      "opens_at": "2026-11-22T01:00:00Z",
      "closes_at": "2026-11-22T08:00:00Z",
      "slots_available": 11,
      "cell_id": "c_45_32",
      "risk_level": "safe",
      "rain_mm_h": 9.0
    },
    "..."
  ],
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
        "distance_m": 4303.0,
        "duration_s": 375.0,
        "risk_penalty": 0.08,
        "max_risk_level": "caution",
        "geometry": {
          "type": "LineString",
          "coordinates": [
            [
              107.618799,
              -6.902236
            ],
            [
              107.618544,
              -6.902237
            ],
            [
              107.618199,
              -6.902242
            ],
            [
              "..."
            ]
          ]
        },
        "reasons": [
          "Rute tercepat",
          "Melewati 1 area Waspada",
          "..."
        ]
      }
    ]
  },
  "meta": {
    "seq": 34,
    "updated_at": "2026-11-22T06:57:38.699273625Z",
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
    "id": "rpt_5",
    "category": "flood",
    "status": "waiting",
    "lat": -6.9175,
    "lon": 107.6093,
    "description": "Genangan 20 cm di Jl. Braga",
    "severity": 2,
    "confirm_count": 0,
    "dispute_count": 0,
    "created_at": "2026-11-22T06:57:40.806621323Z",
    "verified_at": null,
    "expires_at": "2026-11-22T07:00:40.806621323Z"
  },
  "meta": {
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
    "source": "mock",
    "stale": false
  }
}
```

## confirm someone else's report

```sh
curl -s -X POST "$BASE/reports/rpt_5/votes" -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"vote": "confirm"}'
```

`200`

```json
{
  "data": {
    "id": "rpt_5",
    "category": "flood",
    "status": "waiting",
    "lat": -6.9175,
    "lon": 107.6093,
    "description": "Genangan 20 cm di Jl. Braga",
    "severity": 2,
    "confirm_count": 1,
    "dispute_count": 0,
    "created_at": "2026-11-22T06:57:40.806621323Z",
    "verified_at": null,
    "expires_at": "2026-11-22T07:00:40.806621323Z"
  },
  "meta": {
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
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
    "id": "sub_c375b68584b042689d4b",
    "name": "Kampus",
    "lat": -6.8915,
    "lon": 107.6107,
    "radius_m": 1000,
    "created_at": "2026-11-22T06:57:40.826862536Z",
    "risk_level": "safe"
  },
  "meta": {
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
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
      "id": "sub_c375b68584b042689d4b",
      "name": "Kampus",
      "lat": -6.8915,
      "lon": 107.6107,
      "radius_m": 1000,
      "created_at": "2026-11-22T06:57:40.826Z",
      "risk_level": "safe"
    }
  ],
  "meta": {
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
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
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
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
    "seed": 42,
    "tick_seconds": 5,
    "time_scale": 60,
    "tick": 6,
    "active_scenario": null,
    "live_reports": 5,
    "active_disasters": 2,
    "upstreams_down": []
  },
  "meta": {
    "seq": 35,
    "updated_at": "2026-11-22T06:57:40.806621323Z",
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
    "seed": 42,
    "tick_seconds": 5,
    "time_scale": 60,
    "tick": 6,
    "active_scenario": "flood_verified",
    "live_reports": 6,
    "active_disasters": 2,
    "upstreams_down": []
  },
  "meta": {
    "seq": 38,
    "updated_at": "2026-11-22T06:57:40.842523123Z",
    "source": "mock",
    "stale": false
  }
}
```

## delete the subscription

```sh
curl -s -X DELETE "$BASE/subscriptions/sub_c375b68584b042689d4b" -H "Authorization: Bearer $TOKEN"
```

`204`

## logout

```sh
curl -s -X POST "$BASE/auth/logout"  \
  -H 'content-type: application/json' -d '{"refresh_token": "<refresh_token>"}'
```

`204`

