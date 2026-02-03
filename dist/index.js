#!/usr/bin/env node
/**
 * Photo Organizer MCP Server
 * Organizes Google Photos and Google Drive using AI
 */
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema, } from '@modelcontextprotocol/sdk/types.js';
import { exec } from 'child_process';
import { promisify } from 'util';
import * as dotenv from 'dotenv';
import * as path from 'path';
dotenv.config();
const execAsync = promisify(exec);
// Path to Python scripts
const SCRIPTS_PATH = process.env.PHOTO_SCRIPTS_PATH || path.join(process.env.HOME || '', 'drive-photos-organizer');
// Helper to run Python scripts
async function runPythonScript(scriptName, args = []) {
    const scriptPath = path.join(SCRIPTS_PATH, scriptName);
    const command = `cd "${SCRIPTS_PATH}" && python3 "${scriptPath}" ${args.join(' ')}`;
    try {
        const { stdout, stderr } = await execAsync(command, {
            maxBuffer: 10 * 1024 * 1024, // 10MB buffer
            timeout: 300000, // 5 minute timeout
        });
        return stdout + (stderr ? `\n\nWarnings:\n${stderr}` : '');
    }
    catch (error) {
        throw new Error(`Script execution failed: ${error.message}\n${error.stdout || ''}\n${error.stderr || ''}`);
    }
}
// Create MCP server
const server = new Server({
    name: 'photo-organizer-mcp',
    version: '1.0.0',
}, {
    capabilities: {
        tools: {},
    },
});
// Define tools
const tools = [
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
    try {
        let result;
        switch (name) {
            case 'analyze_photos': {
                const scriptArgs = ['--report'];
                if (args.findDuplicates) {
                    scriptArgs.push('--export-duplicates');
                }
                result = await runPythonScript('photos_organizer.py', scriptArgs);
                break;
            }
            case 'organize_photos_by_date': {
                const grouping = args.grouping || 'year';
                const execute = args.execute || false;
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
                const execute = args.execute || false;
                const scriptArgs = ['--organize'];
                if (execute) {
                    scriptArgs.push('--execute');
                }
                result = await runPythonScript('drive_organizer.py', scriptArgs);
                break;
            }
            case 'archive_old_files': {
                const days = args.days || 365;
                const execute = args.execute || false;
                const scriptArgs = ['--archive', '--days', days.toString()];
                if (execute) {
                    scriptArgs.push('--execute');
                }
                result = await runPythonScript('drive_organizer.py', scriptArgs);
                break;
            }
            case 'deduplicate_drive': {
                const execute = args.execute || false;
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
    }
    catch (error) {
        return {
            content: [
                {
                    type: 'text',
                    text: `Error: ${error instanceof Error ? error.message : String(error)}`,
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
