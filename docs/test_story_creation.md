# Manual Test for Step-by-Step Story Creation

This document outlines how to manually test the new step-by-step story creation functionality.

## Prerequisites
- Build the application: `cargo build`
- Have the app ready to run: `cargo run`

## Test Scenario 1: Basic Step-by-Step Story Creation

1. **Start the application**
   ```bash
   cargo run
   ```

2. **Enter input mode**
   - Press `i` to enter input mode

3. **Start interactive story creation**
   - Type: `create s`
   - Press Enter
   - You should see:
     ```
     📖 Starting interactive story creation...
     📝 This will guide you through creating a story step by step.
     📌 Use Esc at any time to cancel.
     📝 Enter story name (or Esc to cancel):
     ```
   - UI mode should show "Creating Story" in the status bar
   - Input area should be green and show "📝 Story Name:"

4. **Enter story name**
   - Type: `My Test Story`
   - Press Enter
   - Should see: `✅ Story name saved`
   - Should see: `📄 Enter story header:`
   - Input prompt should change to "📄 Story Header:"

5. **Enter story header**
   - Type: `This is a test header`
   - Press Enter
   - Should see: `✅ Story header saved`
   - Should see: `📖 Enter story body:`
   - Input prompt should change to "📖 Story Body:"

6. **Enter story body**
   - Type: `This is the body of my test story with some content.`
   - Press Enter
   - Should see: `✅ Story body saved`
   - Should see: `📂 Enter channel (or press Enter for 'general'):`
   - Input prompt should change to "📂 Channel (Enter for 'general'):"

7. **Enter channel (or use default)**
   - Option A: Press Enter (uses "general")
   - Option B: Type: `test-channel` and press Enter
   - Should see: `✅ Story creation complete!`
   - Should see: `Story created successfully in channel 'general'` (or your channel)
   - UI should return to Normal mode

8. **Verify story was created**
   - Type: `ls s local`
   - Should see your new story in the list

## Test Scenario 2: Canceling Story Creation

1. **Start interactive story creation**
   - Press `i` to enter input mode
   - Type: `create s` and press Enter

2. **Cancel during any step**
   - Start entering a story name, then press `Esc`
   - Should see: `❌ Story creation cancelled`
   - Should return to Normal mode

## Test Scenario 3: Input Validation

1. **Start interactive story creation**
   - Press `i`, type `create s`, press Enter

2. **Try empty inputs**
   - For story name: just press Enter without typing anything
   - Should see: `❌ Story name cannot be empty. Please try again:`
   - Should remain in the same step

3. **Continue with valid input**
   - Type a valid name and continue through the process

## Test Scenario 4: Backward Compatibility

1. **Test pipe-separated format still works**
   - Press `i`
   - Type: `create s TestStory|Test Header|Test Body|general`
   - Press Enter
   - Should create story immediately without step-by-step process

## Expected UI Behavior

- **Status Bar**: Should show "Creating Story" mode during interactive creation
- **Input Area**: Should be green during story creation
- **Input Prompts**: Should show appropriate emoji and field name for each step
- **Cursor Position**: Should be positioned correctly after the prompt text
- **Screen Corruption**: Should NOT occur during story creation (main UI polling is paused)

## Commands for Verification

After creating stories, use these commands to verify:
- `ls s local` - List local stories
- `show story <id>` - Show details of a specific story
- `ls ch` - List available channels (if you used custom channels)

## Troubleshooting

If issues occur:
1. Check the application logs for errors
2. Verify file permissions for `stories.json`
3. Use `help` command to see all available commands
4. Try the pipe-separated format as fallback: `create s name|header|body|channel`