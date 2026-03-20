#!/usr/bin/env node

/**
 * Photo Organizer MCP Server
 * Organizes Google Photos and Google Drive using AI
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  Tool,
} from '@modelcontextprotocol/sdk/types.js';
import { execFile } from 'child_process';
import { promisify } from 'util';
import * as dotenv from 'dotenv';
import * as path from 'path';
import * as fs from 'fs';

dotenv.config();

const execFileAsync = promisify(execFile);

// --- Security helpers ---

/** Shell metacharacters that must never appear in arguments. */
const SHELL_METACHAR_RE = /[;|&$`\\(){}<>!\n\r\0]/;

/** Allowed Python script basenames. */
const ALLOWED_SCRIPTS = new Set(['photos_organizer.py', 'drive_organizer.py']);

/** Allowed MCP tool names. */
const ALLOWED_TOOLS = new Set([
  'analyze_photos',
  'organize_photos_by_date',
  'analyze_drive',
  'organize_drive',
  'archive_old_files',
  'deduplicate_drive',
]);

/**
 * Validate that a path is absolute, contains no ".." traversal, and exists on disk.
 */
function validatePath(p: string): string {
  if (!path.isAbsolute(p)) {
    throw new Error('Scripts path must be absolute');
  }
  if (p.includes('..')) {
    throw new Error("Scripts path must not contain '..'");
  }
  const resolved = fs.realpathSync(p); // resolves symlinks, throws if missing
  return resolved;
}

/**
 * Sanitize a single argument — reject if it contains shell metacharacters.
 */
function sanitizeArg(arg: string): void {
  if (SHELL_METACHAR_RE.test(arg)) {
    throw new Error(`Argument contains forbidden characters`);
  }
}

// Validate SCRIPTS_PATH at startup
const SCRIPTS_PATH_RAW = process.env.PHOTO_SCRIPTS_PATH || path.join(process.env.HOME || '', 'drive-photos-organizer');
let SCRIPTS_PATH: string;
try {
  SCRIPTS_PATH = validatePath(SCRIPTS_PATH_RAW);
} catch {
  console.error(`FATAL: Invalid PHOTO_SCRIPTS_PATH: ${SCRIPTS_PATH_RAW}`);
  process.exit(1);
}

// Helper to run Python scripts (uses execFile — no shell interpretation)
async function runPythonScript(scriptName: string, args: string[] = []): Promise<string> {
  // Validate script name against allowlist
  if (!ALLOWED_SCRIPTS.has(scriptName)) {
    throw new Error('Invalid script name');
  }

  // Validate every argument
  for (const arg of args) {
    sanitizeArg(arg);
  }

  const scriptPath = path.join(SCRIPTS_PATH, scriptName);

  // Ensure resolved script path stays inside SCRIPTS_PATH
  const resolvedScript = fs.realpathSync(scriptPath);
  if (!resolvedScript.startsWith(SCRIPTS_PATH + path.sep) && resolvedScript !== SCRIPTS_PATH) {
    throw new Error('Script path escapes the scripts directory');
  }

  try {
    const { stdout, stderr } = await execFileAsync('python3', [resolvedScript, ...args], {
      cwd: SCRIPTS_PATH,
      maxBuffer: 10 * 1024 * 1024, // 10MB buffer
      timeout: 300000, // 5 minute timeout
    });
    if (stderr) {
      console.error(`Script stderr: ${stderr}`);
    }
    return stdout + (stderr ? '\n\n(script produced warnings — see server logs)' : '');
  } catch {
    // Do not expose internal error details to callers
    throw new Error('Script execution failed — check server logs for details');
  }
}

// Create MCP server
const server = new Server(
  {
    name: 'photo-organizer-mcp',
    version: '1.0.0',
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// Define tools
const tools: Tool[] = [
  {
    name: 'analyze_photos',
    description: 'Analyze Google Photos library - get statistics, find duplicates, and generate a report',
    inputSchema: {
      type: 'object',
      properties: {
        findDuplicates: {
          type: 'boolean',
          description: 'Find potential duplicate photos',
          default: true,
        },
      },
    },
  },
  {
    name: 'organize_photos_by_date',
    description: 'Organize Google Photos into albums by date (year or month)',
    inputSchema: {
      type: 'object',
      properties: {
        grouping: {
          type: 'string',
          enum: ['year', 'month'],
          description: 'Group photos by year or month',
          default: 'year',
        },
        execute: {
          type: 'boolean',
          description: 'Actually create albums (false = dry run)',
          default: false,
        },
      },
      required: ['grouping'],
    },
  },
  {
    name: 'analyze_drive',
    description: 'Analyze Google Drive - get file statistics, find duplicates, and generate a report',
    inputSchema: {
      type: 'object',
      properties: {
        findDuplicates: {
          type: 'boolean',
          description: 'Find duplicate files',
          default: true,
        },
      },
    },
  },
  {
    name: 'organize_drive',
    description: 'Organize Google Drive files into folders by type (Documents, Images, Videos, etc.)',
    inputSchema: {
      type: 'object',
      properties: {
        execute: {
          type: 'boolean',
          description: 'Actually organize files (false = dry run)',
          default: false,
        },
      },
    },
  },
  {
    name: 'archive_old_files',
    description: 'Move old Drive files (older than specified days) to an Archive folder',
    inputSchema: {
      type: 'object',
      properties: {
        days: {
          type: 'number',
          description: 'Archive files older than this many days',
          default: 365,
        },
        execute: {
          type: 'boolean',
          description: 'Actually archive files (false = dry run)',
          default: false,
        },
      },
    },
  },
  {
    name: 'deduplicate_drive',
    description: 'Remove exact duplicate files from Google Drive',
    inputSchema: {
      type: 'object',
      properties: {
        execute: {
          type: 'boolean',
          description: 'Actually delete duplicates (false = dry run)',
          default: false,
        },
      },
    },
  },
];

// Handle list_tools request
server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools,
}));

// Handle call_tool request
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  // Validate tool name against allowlist before dispatching
  if (!ALLOWED_TOOLS.has(name)) {
    return {
      content: [{ type: 'text', text: 'Error: Unknown or disallowed tool' }],
      isError: true,
    };
  }

  try {
    let result: string;

    switch (name) {
      case 'analyze_photos': {
        const scriptArgs = ['--report'];
        if ((args as any).findDuplicates) {
          scriptArgs.push('--export-duplicates');
        }
        result = await runPythonScript('photos_organizer.py', scriptArgs);
        break;
      }

      case 'organize_photos_by_date': {
        const grouping = (args as any).grouping || 'year';
        const execute = (args as any).execute || false;

        const scriptArgs = [grouping === 'year' ? '--by-year' : '--by-month'];
        if (execute) {
          scriptArgs.push('--execute');
        }

        result = await runPythonScript('photos_organizer.py', scriptArgs);
        break;
      }

      case 'analyze_drive': {
        const scriptArgs = ['--report'];
        result = await runPythonScript('drive_organizer.py', scriptArgs);
        break;
      }

      case 'organize_drive': {
        const execute = (args as any).execute || false;
        const scriptArgs = ['--organize'];
        if (execute) {
          scriptArgs.push('--execute');
        }
        result = await runPythonScript('drive_organizer.py', scriptArgs);
        break;
      }

      case 'archive_old_files': {
        const days = (args as any).days || 365;
        const execute = (args as any).execute || false;

        const scriptArgs = ['--archive', '--days', days.toString()];
        if (execute) {
          scriptArgs.push('--execute');
        }

        result = await runPythonScript('drive_organizer.py', scriptArgs);
        break;
      }

      case 'deduplicate_drive': {
        const execute = (args as any).execute || false;
        const scriptArgs = ['--dedupe'];
        if (execute) {
          scriptArgs.push('--execute');
        }
        result = await runPythonScript('drive_organizer.py', scriptArgs);
        break;
      }

      default:
        throw new Error(`Unknown tool: ${name}`);
    }

    return {
      content: [
        {
          type: 'text',
          text: result,
        },
      ],
    };
  } catch (error) {
    // Log full error server-side for debugging
    console.error('Tool execution error:', error);
    // Return only a safe message to the caller
    const safeMessage = error instanceof Error ? error.message : 'An unexpected error occurred';
    return {
      content: [
        {
          type: 'text',
          text: `Error: ${safeMessage}`,
        },
      ],
      isError: true,
    };
  }
});

// Start server
async function main() {
  console.error('Starting Photo Organizer MCP server...');
  console.error(`Python scripts path: ${SCRIPTS_PATH}`);

  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('Photo Organizer MCP server running!');
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});
