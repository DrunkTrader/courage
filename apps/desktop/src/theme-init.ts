const savedTheme = window.localStorage.getItem('courage-theme')
if (savedTheme === 'light' || savedTheme === 'dark' || savedTheme === 'rusty') {
  document.documentElement.dataset.theme = savedTheme
}
