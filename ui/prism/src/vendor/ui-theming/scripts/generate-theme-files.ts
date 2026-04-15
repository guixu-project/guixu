/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { generateCssContent } from "./generate-css-content";
import { generateTsContent } from "./generate-ts-content";
import { saveContentToFile } from "./save-to-file";

const cssContent = generateCssContent();
const tsContent = generateTsContent();

saveContentToFile(cssContent, "theme.css");
saveContentToFile(tsContent, "index.ts");
