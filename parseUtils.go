package main

import (
	"errors"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"

	"golang.org/x/net/html"
	"golang.org/x/net/html/atom"
)

var suffixHTMLMatch = regexp.MustCompile(`(?i)(.+)index.html?$`)

func getHrefsFromHTML(body io.ReadCloser) []string {
	var hrefs []string
	tokenizer := html.NewTokenizer(body)
	for {
		token := tokenizer.Next()

		switch {
		case token == html.ErrorToken:
			return hrefs
		case token == html.StartTagToken:
			current := tokenizer.Token()

			if current.DataAtom == atom.A {
				for _, a := range current.Attr {
					if a.Key == "href" && a.Val != "../" {
						// fmt.Println(a.Val)
						hrefs = append(hrefs, a.Val)
						break
					}
				}
			}
		}
	}
}

func urlBuilder(base *url.URL, href string) (*url.URL, error) {
	res, err := base.Parse(href)
	return res, err
}

// HasTrailingSlash whether a URL has trailing slash
func HasTrailingSlash(base *url.URL) bool {
	return strings.HasSuffix(base.Path, "/")
}

func validateEnterpoint(enterpoint *url.URL) {
	if !HasTrailingSlash(enterpoint) {
		panic(ErrEnterpointHasNoTrailingSlash)
	}
}

func addTrailingSlashForAbsURL(base *url.URL) {
	// if trailing slash exists, it will sliently returns
	if HasTrailingSlash(base) {
		return
	}
	base.Path = base.Path + "/"
}

// IsHTML checks whether first value == 'text/html' in Content-Type header
func IsHTML(header http.Header) bool {
	contentTypeHeader := header.Get("Content-Type")
	contentType := strings.SplitN(contentTypeHeader, ";", 2)[0]
	if contentType == "text/html" {
		return true
	}
	return false
}

// IsRedirect checks whether a response when redirect to other locs
func IsRedirect(resp *http.Response) bool {
	switch resp.StatusCode {
	case 301:
		fallthrough
	case 302:
		fallthrough
	case 303:
		fallthrough
	case 307:
		fallthrough
	case 308:
		return true
	}
	return false
}

func getRedirectLocation(resp *http.Response) (*url.URL, error) {
	location, err := resp.Location()
	if err != nil {
		return nil, err
	}
	matched := suffixHTMLMatch.MatchString(location.Path)
	if !matched {
		return location, nil
	}
	newPath := suffixHTMLMatch.ReplaceAllString(location.Path, "$1")
	location.Path = newPath
	return location, nil
}

func isURLOutOfBoundary(url *url.URL) error {
	if url.Hostname() != boundaryHost {
		return errors.New("host out of boundary")
	}
	if !strings.HasPrefix(url.Path, boundaryPrefix) {
		return errors.New("path out of boundary")
	}
	return nil
}

func getFileRelPath(url *url.URL) string {
	return strings.TrimPrefix(url.Path, boundaryPrefix)
}
