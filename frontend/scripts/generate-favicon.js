#!/usr/bin/env node

/**
 * Generate favicon PNGs from SVG
 * Generates multiple sizes for different devices
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateFavicons() {
  try {
    const sharp = (await import('sharp')).default;
    
    const svgPath = path.join(__dirname, '../public/favicon.svg');
    const publicDir = path.join(__dirname, '../public');
    
    const svgBuffer = fs.readFileSync(svgPath);
    
    // Generate different sizes
    const sizes = [
      { size: 16, name: 'favicon-16x16.png' },
      { size: 32, name: 'favicon-32x32.png' },
      { size: 48, name: 'favicon-48x48.png' },
      { size: 64, name: 'favicon-64x64.png' },
      { size: 128, name: 'favicon-128x128.png' },
      { size: 180, name: 'apple-touch-icon.png' },
      { size: 192, name: 'android-chrome-192x192.png' },
      { size: 512, name: 'android-chrome-512x512.png' },
      { size: 512, name: 'favicon.png' },  // Main large favicon
    ];
    
    for (const { size, name } of sizes) {
      await sharp(svgBuffer)
        .resize(size, size)
        .png()
        .toFile(path.join(publicDir, name));
      
      console.log(`✅ Generated ${name} (${size}x${size})`);
    }
    
    // Generate ICO file info (user needs to combine manually or use online tool)
    console.log('\n📝 Note: For favicon.ico, combine 16x16, 32x32, and 48x48 PNGs');
    console.log('   Use: https://icoconvert.com/ or similar tool\n');
    
    console.log('✅ All favicons generated successfully!');
  } catch (error) {
    if (error.code === 'ERR_MODULE_NOT_FOUND') {
      console.log(`
📸 To generate favicons, install sharp first:
  npm install sharp --save-dev
  node scripts/generate-favicon.js
      `);
    } else {
      console.error('Error generating favicons:', error);
    }
  }
}

generateFavicons();



