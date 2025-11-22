#!/usr/bin/env node

/**
 * Generate OG image PNG from SVG
 * 
 * Usage: node scripts/generate-og-image.js
 * 
 * Requirements:
 *   npm install sharp --save-dev
 * 
 * Or use an online tool like:
 *   - https://cloudconvert.com/svg-to-png
 *   - https://svgtopng.com/
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateOgImage() {
  try {
    // Try to use sharp if available
    const sharp = (await import('sharp')).default;
    
    const svgPath = path.join(__dirname, '../public/og-image.svg');
    const pngPath = path.join(__dirname, '../public/og-image.png');
    
    const svgBuffer = fs.readFileSync(svgPath);
    
    await sharp(svgBuffer)
      .resize(1200, 630)
      .png()
      .toFile(pngPath);
    
    console.log('✅ Generated og-image.png successfully!');
  } catch (error) {
    if (error.code === 'ERR_MODULE_NOT_FOUND') {
      console.log(`
📸 To generate the OG image PNG, you have two options:

Option 1: Install sharp and run this script
  npm install sharp --save-dev
  node scripts/generate-og-image.js

Option 2: Convert manually
  1. Open public/og-image.svg in a browser
  2. Use an online converter like https://cloudconvert.com/svg-to-png
  3. Save as public/og-image.png (1200x630 pixels)

Note: SVG works for most platforms, but some (like Twitter) require PNG.
      `);
    } else {
      console.error('Error generating OG image:', error);
    }
  }
}

generateOgImage();
