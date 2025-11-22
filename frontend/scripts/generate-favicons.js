#!/usr/bin/env node

/**
 * Generate favicon PNG files from SVG
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
    const svgBuffer = fs.readFileSync(svgPath);
    
    // Generate different sizes
    const sizes = [
      { size: 16, name: 'favicon-16x16.png' },
      { size: 32, name: 'favicon-32x32.png' },
      { size: 48, name: 'favicon-48x48.png' },
      { size: 180, name: 'apple-touch-icon.png' },
      { size: 192, name: 'android-chrome-192x192.png' },
      { size: 512, name: 'android-chrome-512x512.png' },
      { size: 512, name: 'favicon.png' }, // Large PNG favicon
    ];
    
    for (const { size, name } of sizes) {
      const outputPath = path.join(__dirname, '../public', name);
      await sharp(svgBuffer)
        .resize(size, size)
        .png()
        .toFile(outputPath);
      console.log(`✅ Generated ${name} (${size}x${size})`);
    }
    
    console.log('\n🎉 All favicons generated successfully!');
  } catch (error) {
    if (error.code === 'ERR_MODULE_NOT_FOUND') {
      console.log('Please install sharp: npm install sharp --save-dev');
    } else {
      console.error('Error:', error);
    }
  }
}

generateFavicons();

