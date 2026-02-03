# Photo Organizer MCP Server

**AI-powered organization for Google Photos and Google Drive.**

This MCP server enables AI agents like Claude to automatically organize your Google Photos and Drive files. Perfect for decluttering cloud storage, finding duplicates, and maintaining organized photo albums.

## Features

### Google Photos
- 📊 **Analyze Library**: Get statistics and insights about your photo collection
- 🔍 **Find Duplicates**: Identify potential duplicate photos
- 📅 **Auto-Organize**: Create albums by year or month
- 📈 **Reports**: Generate detailed organization reports

### Google Drive
- 📂 **Auto-Organize**: Sort files into folders by type (Documents, Images, Videos, etc.)
- 🗄️ **Archive Old Files**: Move old files to Archive folder
- 🔄 **Deduplicate**: Find and remove exact duplicate files
- 📊 **Analytics**: Get file statistics and storage insights

## Installation

```bash
# Install from NPM
npm install -g photo-organizer-mcp

# Or clone and build
git clone https://github.com/ExpertVagabond/photo-organizer-mcp
cd photo-organizer-mcp
npm install
npm run build
```

## Setup

### 1. Google Cloud Credentials

You need Google Cloud credentials to access Photos and Drive APIs:

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project
3. Enable **Google Photos Library API** and **Google Drive API**
4. Create OAuth 2.0 credentials
5. Download `credentials.json`

### 2. Python Scripts

This MCP server wraps existing Python organizer scripts. Set the path:

```bash
export PHOTO_SCRIPTS_PATH="/path/to/drive-photos-organizer"
```

Or add to `.env`:
```
PHOTO_SCRIPTS_PATH=/Users/yourname/drive-photos-organizer
```

### 3. Configure Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "photo-organizer": {
      "command": "photo-organizer-mcp",
      "env": {
        "PHOTO_SCRIPTS_PATH": "/path/to/drive-photos-organizer"
      }
    }
  }
}
```

## Usage Examples

### With Claude

**"Analyze my Google Photos and find duplicates"**
```
Claude will use the analyze_photos tool to scan your library
```

**"Organize my photos into albums by year"**
```
Claude will create year-based albums (dry run first, then execute)
```

**"Clean up my Google Drive by organizing files into folders"**
```
Claude will sort files by type into organized folders
```

**"Archive all Drive files older than 2 years"**
```
Claude will move old files to an Archive folder
```

**"Find and remove duplicate files from my Drive"**
```
Claude will identify and remove exact duplicates
```

## Available Tools

### Photo Tools

1. **analyze_photos** - Get photo library statistics
   ```json
   {
     "findDuplicates": true
   }
   ```

2. **organize_photos_by_date** - Create date-based albums
   ```json
   {
     "grouping": "year",  // or "month"
     "execute": false     // true to actually create albums
   }
   ```

### Drive Tools

3. **analyze_drive** - Get Drive statistics

4. **organize_drive** - Sort files into folders
   ```json
   {
     "execute": false  // true to actually organize
   }
   ```

5. **archive_old_files** - Move old files to Archive
   ```json
   {
     "days": 730,      // Archive files older than this
     "execute": false
   }
   ```

6. **deduplicate_drive** - Remove duplicate files
   ```json
   {
     "execute": false  // true to actually delete
   }
   ```

## Safety Features

- **Dry Run by Default**: All operations default to dry run mode
- **Explicit Execution**: Must set `execute: true` to make changes
- **Detailed Reports**: See exactly what will happen before executing
- **Non-Destructive**: Organizes and archives, doesn't delete (except deduplication)

## Monetization (Pro Version)

Upgrade for advanced features:

### Free Tier
- 50 operations per month
- Basic organization
- Manual execution required

### Pro ($10/month)
- Unlimited operations
- Scheduled auto-organization
- Advanced duplicate detection
- Priority support

### Enterprise ($50/month)
- White-label branding
- Team management
- Custom rules engine
- API access

## Technical Details

- Built with TypeScript and Model Context Protocol SDK
- Wraps Python scripts for Google API integration
- Async operation with progress reporting
- Handles large libraries (10,000+ photos)

## Contributing

Contributions welcome! Please open issues or PRs on GitHub.

## License

MIT License - see LICENSE file

## Author

ExpertVagabond - https://github.com/ExpertVagabond

---

**Need help?** Contact: hello@expertvagabond.com
