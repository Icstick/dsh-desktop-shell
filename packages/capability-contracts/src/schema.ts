/**
 * Embedded copies of the normative protocol schemas.
 *
 * These are byte-for-byte copies of the spec files (verified by
 * test/validate.test.ts "embedded schemas match specs"): the runtime
 * validator compiles from these embedded documents so the library needs no
 * filesystem access, while the tests pin them to the live specs.
 *
 * Source files:
 *   specs/protocol/protocol-coordinate.schema.json
 *   specs/protocol/envelope.schema.json
 *   specs/protocol/capability-lease.schema.json
 */

/** Registry keyed by schema file basename, for `./file.schema.json` \$ref resolution. */
export type SchemaRegistry = ReadonlyMap<string, Record<string, unknown>>;

const coordinateSchema = {
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://dsh-desktop.local/specs/protocol/protocol-coordinate.schema.json",
  "title": "ProtocolCoordinate",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "apiVersion",
    "kind"
  ],
  "properties": {
    "apiVersion": {
      "type": "string",
      "pattern": "^[a-z0-9.-]+/v[0-9]+(alpha[0-9]+|beta[0-9]+)?$"
    },
    "kind": {
      "type": "string",
      "pattern": "^[A-Z][A-Za-z0-9]+$"
    }
  }
} as const;

const envelopeSchema = {
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://dsh-desktop.local/specs/protocol/envelope.schema.json",
  "title": "ProtocolEnvelope",
  "type": "object",
  "additionalProperties": false,
  "$defs": {
    "requirement": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "coordinate",
        "required"
      ],
      "properties": {
        "coordinate": {
          "$ref": "./protocol-coordinate.schema.json"
        },
        "required": {
          "type": "boolean"
        }
      }
    },
    "helloPayload": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "instanceId",
        "supports",
        "requires"
      ],
      "properties": {
        "instanceId": {
          "type": "string",
          "minLength": 8,
          "maxLength": 128
        },
        "supports": {
          "type": "array",
          "items": {
            "$ref": "./protocol-coordinate.schema.json"
          },
          "uniqueItems": true
        },
        "requires": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/requirement"
          },
          "uniqueItems": true
        }
      }
    },
    "unavailableCapability": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "coordinate",
        "reason"
      ],
      "properties": {
        "coordinate": {
          "$ref": "./protocol-coordinate.schema.json"
        },
        "reason": {
          "enum": [
            "unavailable",
            "unsupported_version",
            "policy_denied",
            "provider_failed"
          ]
        }
      }
    },
    "agreementPayload": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "activationId",
        "granted",
        "unavailable"
      ],
      "properties": {
        "activationId": {
          "type": "string",
          "minLength": 1,
          "maxLength": 128
        },
        "granted": {
          "type": "array",
          "items": {
            "$ref": "./protocol-coordinate.schema.json"
          },
          "uniqueItems": true
        },
        "unavailable": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/unavailableCapability"
          },
          "uniqueItems": true
        },
        "leaseConstraints": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "maxSeconds": {
              "type": "integer",
              "minimum": 1
            },
            "approvalRequired": {
              "type": "boolean"
            }
          }
        }
      }
    }
  },
  "required": [
    "protocol",
    "id",
    "kind",
    "participant",
    "timestamp",
    "generation"
  ],
  "properties": {
    "protocol": {
      "const": "interop.dsh-desktop.local/v1alpha1"
    },
    "id": {
      "type": "string",
      "minLength": 8,
      "maxLength": 128
    },
    "kind": {
      "enum": [
        "Hello",
        "Agreement",
        "Invocation",
        "Result",
        "Event"
      ]
    },
    "replyTo": {
      "type": "string",
      "minLength": 8,
      "maxLength": 128
    },
    "participant": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "component",
        "facet"
      ],
      "properties": {
        "component": {
          "type": "string",
          "minLength": 1
        },
        "facet": {
          "type": "string",
          "minLength": 1
        },
        "activationId": {
          "type": "string",
          "minLength": 1,
          "maxLength": 128
        }
      }
    },
    "timestamp": {
      "type": "string",
      "format": "date-time"
    },
    "generation": {
      "type": "integer",
      "minimum": 0
    },
    "capability": {
      "$ref": "./protocol-coordinate.schema.json"
    },
    "method": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9._-]+$"
    },
    "payload": {
      "type": "object"
    },
    "error": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "code",
        "message",
        "retryable",
        "correlationId"
      ],
      "properties": {
        "code": {
          "enum": [
            "UNAVAILABLE",
            "UNAUTHORIZED",
            "UNSUPPORTED_VERSION",
            "NOT_PROCESS_OWNER",
            "USER_GESTURE_REQUIRED",
            "USER_DENIED",
            "STALE_GENERATION",
            "MALFORMED_MESSAGE",
            "CONFLICT",
            "TIMEOUT",
            "SAFE_STOP"
          ]
        },
        "message": {
          "type": "string",
          "maxLength": 512
        },
        "retryable": {
          "type": "boolean"
        },
        "correlationId": {
          "type": "string",
          "minLength": 8,
          "maxLength": 128
        }
      }
    }
  },
  "allOf": [
    {
      "if": {
        "properties": {
          "kind": {
            "const": "Hello"
          }
        }
      },
      "then": {
        "required": [
          "payload"
        ],
        "properties": {
          "payload": {
            "$ref": "#/$defs/helloPayload"
          }
        },
        "not": {
          "anyOf": [
            {
              "required": [
                "replyTo"
              ]
            },
            {
              "required": [
                "capability"
              ]
            },
            {
              "required": [
                "method"
              ]
            },
            {
              "required": [
                "error"
              ]
            }
          ]
        }
      }
    },
    {
      "if": {
        "properties": {
          "kind": {
            "const": "Agreement"
          }
        }
      },
      "then": {
        "required": [
          "replyTo",
          "payload"
        ],
        "properties": {
          "payload": {
            "$ref": "#/$defs/agreementPayload"
          }
        },
        "not": {
          "anyOf": [
            {
              "required": [
                "capability"
              ]
            },
            {
              "required": [
                "method"
              ]
            },
            {
              "required": [
                "error"
              ]
            }
          ]
        }
      }
    },
    {
      "if": {
        "properties": {
          "kind": {
            "const": "Invocation"
          }
        }
      },
      "then": {
        "required": [
          "capability",
          "method",
          "payload"
        ],
        "not": {
          "anyOf": [
            {
              "required": [
                "replyTo"
              ]
            },
            {
              "required": [
                "error"
              ]
            }
          ]
        }
      }
    },
    {
      "if": {
        "properties": {
          "kind": {
            "const": "Result"
          }
        }
      },
      "then": {
        "required": [
          "replyTo",
          "capability",
          "method"
        ],
        "oneOf": [
          {
            "required": [
              "payload"
            ],
            "not": {
              "required": [
                "error"
              ]
            }
          },
          {
            "required": [
              "error"
            ],
            "not": {
              "required": [
                "payload"
              ]
            }
          }
        ]
      }
    },
    {
      "if": {
        "properties": {
          "kind": {
            "const": "Event"
          }
        }
      },
      "then": {
        "required": [
          "capability",
          "method",
          "payload"
        ],
        "not": {
          "required": [
            "error"
          ]
        }
      }
    }
  ]
} as const;

const leaseSchema = {
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://dsh-desktop.local/specs/protocol/capability-lease.schema.json",
  "title": "CapabilityLease",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "leaseId",
    "participantId",
    "activationId",
    "capability",
    "owner",
    "generation",
    "scope"
  ],
  "properties": {
    "leaseId": {
      "type": "string",
      "minLength": 8
    },
    "participantId": {
      "type": "string",
      "minLength": 1
    },
    "activationId": {
      "type": "string",
      "minLength": 1
    },
    "capability": {
      "$ref": "./protocol-coordinate.schema.json"
    },
    "owner": {
      "type": "string",
      "minLength": 1
    },
    "generation": {
      "type": "integer",
      "minimum": 0
    },
    "scope": {
      "type": "object",
      "additionalProperties": false,
      "minProperties": 1,
      "properties": {
        "sessionId": {
          "type": "string"
        },
        "workspace": {
          "type": "string"
        },
        "domains": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "uniqueItems": true
        },
        "resources": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "uniqueItems": true
        }
      }
    },
    "expiresAt": {
      "type": "string",
      "format": "date-time"
    }
  }
} as const;

/** All embedded schemas keyed by their file basename (resolvable \$ref targets). */
export const schemaRegistry: SchemaRegistry = new Map([
  ["protocol-coordinate.schema.json", coordinateSchema as Record<string, unknown>],
  ["envelope.schema.json", envelopeSchema as Record<string, unknown>],
  ["capability-lease.schema.json", leaseSchema as Record<string, unknown>],
]);
